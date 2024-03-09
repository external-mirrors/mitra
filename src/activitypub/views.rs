use actix_web::{
    get,
    post,
    web,
    http::header as http_header,
    HttpRequest,
    HttpResponse,
    Scope,
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_activitypub::identifiers::{
    local_actor_id,
    local_object_id,
    local_object_replies,
    post_object_id,
    LocalActorCollection,
};
use mitra_adapters::authority::Authority;
use mitra_config::Config;
use mitra_federation::{
    constants::AP_MEDIA_TYPE,
    http_server::is_activitypub_request,
};
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    emojis::queries::get_local_emoji_by_name,
    posts::helpers::{add_related_posts, can_view_post},
    posts::queries::{get_post_by_id, get_posts_by_author, get_thread},
    profiles::types::PaymentOption,
    users::queries::{get_user_by_id, get_user_by_name},
};
use mitra_utils::{
    caip2::ChainId,
    http_digest::get_sha256_digest,
};
use mitra_validators::errors::ValidationError;

use crate::errors::HttpError;
use crate::web_client::urls::{
    get_post_page_url,
    get_profile_page_url,
    get_subscription_page_url,
    get_tag_page_url,
};

use super::actors::builders::{
    build_instance_actor,
    build_local_actor,
    sign_object_fep_ef61,
};
use super::authentication::verify_signed_c2s_activity;
use super::builders::{
    announce::build_announce,
    collection::{
        OrderedCollection,
        OrderedCollectionPage,
    },
    create_note::build_create_note,
    emoji::build_emoji,
    note::build_note,
    proposal::build_proposal,
    update_person::{
        forward_update_person,
        is_update_person_activity,
        validate_update_person_c2s,
    },
};
use super::receiver::{receive_activity, InboxError, HandlerError};

#[derive(Deserialize)]
pub struct ObjectQueryParams {
    #[serde(default)]
    fep_ef61: bool,
}

#[get("")]
async fn actor_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    username: web::Path<String>,
    query_params: web::Query<ObjectQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    // Do not redirect when viewing FEP-ef61 representation
    if !is_activitypub_request(request.headers()) && !query_params.fep_ef61 {
        let page_url = get_profile_page_url(
            &config.instance_url(),
            &user.profile.username,
        );
        let response = HttpResponse::Found()
            .append_header((http_header::LOCATION, page_url))
            .finish();
        return Ok(response);
    };
    let authority = Authority::from_user(
        &config.instance_url(),
        &user,
        query_params.fep_ef61,
    );
    let actor = build_local_actor(
        &config.instance_url(),
        &authority,
        &user,
    )?;
    let mut actor_value = serde_json::to_value(actor)
        .expect("actor should be serializable");
    if authority.is_fep_ef61() {
        actor_value = sign_object_fep_ef61(
            &authority,
            &user,
            &actor_value,
            None,
        );
    };
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(actor_value);
    Ok(response)
}

#[post("/inbox")]
async fn inbox(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    username: web::Path<String>,
    request: HttpRequest,
    request_body: web::Bytes,
) -> Result<HttpResponse, HttpError> {
    if !config.federation.enabled {
        return Err(HttpError::PermissionError);
    };
    let activity: JsonValue = serde_json::from_slice(&request_body)
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_digest = get_sha256_digest(&request_body);
    drop(request_body);

    log::debug!("received activity: {}", activity);
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    log::info!("received in {}: {}", request.uri().path(), activity_type);

    let db_client = &mut **get_database_client(&db_pool).await?;
    let _user = get_user_by_name(db_client, &username).await?;
    receive_activity(
        &config,
        db_client,
        &request,
        &activity,
        activity_digest,
    ).await
        .map_err(|error| {
            // TODO: preserve original error text in DatabaseError
            if let InboxError::HandlerError(
                HandlerError::DatabaseError(
                    DatabaseError::DatabaseClientError(ref pg_error))) = error
            {
                log::error!("database client error: {}", pg_error);
            };
            log::warn!(
                "failed to process activity ({}): {}",
                error,
                activity,
            );
            error
        })?;
    Ok(HttpResponse::Accepted().finish())
}

#[derive(Deserialize)]
pub struct CollectionQueryParams {
    page: Option<bool>,
}

#[get("/outbox")]
async fn outbox(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    let instance = config.instance();
    let actor_id = local_actor_id(&instance.url(), &username);
    let collection_id = LocalActorCollection::Outbox.of(&actor_id);
    let first_page_id = format!("{}?page=true", collection_id);
    if query_params.page.is_none() {
        let collection = OrderedCollection::new(
            collection_id,
            Some(first_page_id),
            None,
            false,
        );
        let response = HttpResponse::Ok()
            .content_type(AP_MEDIA_TYPE)
            .json(collection);
        return Ok(response);
    };
    // Posts are ordered by creation date
    let mut posts = get_posts_by_author(
        db_client,
        &user.id,
        None, // include only public posts
        true, // include replies
        true, // include reposts
        false, // not only pinned
        false, // not only media
        None,
        OrderedCollectionPage::DEFAULT_SIZE,
    ).await?;
    add_related_posts(db_client, posts.iter_mut().collect()).await?;
    let activities = posts.iter().map(|post| {
        if post.repost_of_id.is_some() {
            let activity = build_announce(&instance.url(), post);
            serde_json::to_value(activity)
                .expect("activity should be serializable")
        } else {
            let activity = build_create_note(
                &instance.hostname(),
                &instance.url(),
                post,
                config.federation.fep_e232_enabled,
            );
            serde_json::to_value(activity)
                .expect("activity should be serializable")
        }
    }).collect();
    let collection_page = OrderedCollectionPage::new(
        first_page_id,
        activities,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection_page);
    Ok(response)
}

#[post("/outbox")]
async fn outbox_client_to_server(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    activity: web::Json<JsonValue>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let instance = config.instance();
    let outgoing_activity = match is_update_person_activity(&activity) {
        true => {
            let user = validate_update_person_c2s(
                db_client,
                &instance,
                &activity,
            ).await.map_err(|_| ValidationError("invalid activity"))?;
            verify_signed_c2s_activity(&user.profile, &activity)
                .map_err(|_| ValidationError("invalid integrity proof"))?;
            forward_update_person(
                db_client,
                &user,
                &activity,
            ).await?
        },
        false => return Err(ValidationError("unsupported activity type").into()),
    };
    outgoing_activity.enqueue(db_client).await?;
    Ok(HttpResponse::Accepted().finish())
}

#[get("/followers")]
async fn followers_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    };
    let actor_id = local_actor_id(&config.instance_url(), &username);
    let collection_id = LocalActorCollection::Followers.of(&actor_id);
    let collection = OrderedCollection::new(
        collection_id,
        None,
        Some(user.profile.follower_count),
        false,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/following")]
async fn following_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    };
    let actor_id = local_actor_id(&config.instance_url(), &username);
    let collection_id = LocalActorCollection::Following.of(&actor_id);
    let collection = OrderedCollection::new(
        collection_id,
        None,
        Some(user.profile.following_count),
        false,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/subscribers")]
async fn subscribers_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    if query_params.page.is_some() {
        // Subscriber list is hidden
        return Err(HttpError::PermissionError);
    };
    let actor_id = local_actor_id(&config.instance_url(), &username);
    let collection_id = LocalActorCollection::Subscribers.of(&actor_id);
    let collection = OrderedCollection::new(
        collection_id,
        None,
        Some(user.profile.subscriber_count),
        false,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/collections/featured")]
async fn featured_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    let instance = config.instance();
    let actor_id = local_actor_id(&instance.url(), &username);
    let collection_id = LocalActorCollection::Featured.of(&actor_id);
    let first_page_id = format!("{}?page=true", collection_id);
    if query_params.page.is_none() {
        let collection = OrderedCollection::new(
            collection_id,
            Some(first_page_id),
            None,
            true,
        );
        let response = HttpResponse::Ok()
            .content_type(AP_MEDIA_TYPE)
            .json(collection);
        return Ok(response);
    };
    let mut posts = get_posts_by_author(
        db_client,
        &user.id,
        None, // include only public posts
        true, // include replies
        false, // exclude reposts
        true, // only pinned
        false, // not only media
        None,
        OrderedCollectionPage::DEFAULT_SIZE,
    ).await?;
    add_related_posts(db_client, posts.iter_mut().collect()).await?;
    let authority = Authority::server(&instance.url());
    let objects = posts.iter().map(|post| {
        let note = build_note(
            &instance.hostname(),
            &instance.url(),
            &authority,
            post,
            config.federation.fep_e232_enabled,
            false,
        );
        serde_json::to_value(note)
            .expect("note should be serializable")
    }).collect();
    let collection_page = OrderedCollectionPage::new(
        first_page_id,
        objects,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection_page);
    Ok(response)
}

#[get("/proposals/{chain_id}")]
async fn proposal_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    path: web::Path<(String, ChainId)>,
) -> Result<HttpResponse, HttpError> {
    let (username, chain_id) = path.into_inner();
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    let payment_option = user.profile.payment_options
        .inner().iter()
        .find(|option| option.chain_id() == Some(&chain_id))
        .ok_or(HttpError::NotFoundError("proposal"))?;
    let payment_info = match payment_option {
        PaymentOption::MoneroSubscription(payment_info)
            if is_activitypub_request(request.headers()) => payment_info,
        PaymentOption::EthereumSubscription(_) |
            PaymentOption::MoneroSubscription(_) =>
        {
            // Ethereum subscription proposals are not implemented, redirect
            let page_url = get_subscription_page_url(
                &config.instance_url(),
                &user.profile.username,
            );
            let response = HttpResponse::Found()
                .append_header((http_header::LOCATION, page_url))
                .finish();
            return Ok(response);
        },
        _ => return Err(HttpError::InternalError),
    };
    let proposal = build_proposal(
        &config.instance_url(),
        &user.profile.username,
        payment_info,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(proposal);
    Ok(response)
}

pub fn actor_scope() -> Scope {
    web::scope("/users/{username}")
        .service(actor_view)
        .service(inbox)
        .service(outbox)
        .service(outbox_client_to_server)
        .service(followers_collection)
        .service(following_collection)
        .service(subscribers_collection)
        .service(featured_collection)
        .service(proposal_view)
}

#[get("")]
async fn instance_actor_view(
    config: web::Data<Config>,
) -> Result<HttpResponse, HttpError> {
    let actor = build_instance_actor(&config.instance())
        .map_err(|_| HttpError::InternalError)?;
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(actor);
    Ok(response)
}

#[post("/inbox")]
async fn instance_actor_inbox(
    config: web::Data<Config>,
    activity: web::Json<JsonValue>,
) -> Result<HttpResponse, HttpError> {
    if !config.federation.enabled {
        return Err(HttpError::PermissionError);
    };
    log::info!(
        "received in instance inbox: {}",
        activity["type"].as_str().unwrap_or("Unknown"),
    );
    Ok(HttpResponse::Accepted().finish())
}

pub fn instance_actor_scope() -> Scope {
    web::scope("/actor")
        .service(instance_actor_view)
        .service(instance_actor_inbox)
}

#[get("/objects/{object_id}")]
pub async fn object_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    internal_object_id: web::Path<Uuid>,
    query_params: web::Query<ObjectQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let internal_object_id = internal_object_id.into_inner();
    let instance = config.instance();
    // Try to find local post by ID,
    // return 404 if not found, or not public, or it is a repost
    let mut post = get_post_by_id(db_client, &internal_object_id).await?;
    if !post.is_local() || !can_view_post(db_client, None, &post).await? {
        return Err(HttpError::NotFoundError("post"));
    };
    if !is_activitypub_request(request.headers()) && !query_params.fep_ef61 {
        let page_url = get_post_page_url(&instance.url(), &post.id);
        let response = HttpResponse::Found()
            .append_header((http_header::LOCATION, page_url))
            .finish();
        return Ok(response);
    };
    add_related_posts(db_client, vec![&mut post]).await?;
    let user = get_user_by_id(db_client, &post.author.id).await?;
    let authority = Authority::from_user(
        &instance.url(),
        &user,
        query_params.fep_ef61,
    );
    let object = build_note(
        &instance.hostname(),
        &instance.url(),
        &authority,
        &post,
        config.federation.fep_e232_enabled,
        true,
    );
    let mut object_value = serde_json::to_value(object)
        .expect("actor should be serializable");
    if authority.is_fep_ef61() {
        object_value = sign_object_fep_ef61(
            &authority,
            &user,
            &object_value,
            None,
        );
    };
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(object_value);
    Ok(response)
}

#[get("/objects/{object_id}/replies")]
pub async fn replies_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    internal_object_id: web::Path<Uuid>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let internal_object_id = internal_object_id.into_inner();
    let posts = get_thread(db_client, &internal_object_id, None).await?;
    let post = posts.iter().find(|post| post.id == internal_object_id)
        .expect("get_thread return value should contain target post");
    if !post.is_local() || !can_view_post(db_client, None, post).await? {
        return Err(HttpError::NotFoundError("post"));
    };
    let instance = config.instance();
    let object_id = local_object_id(&instance.url(), &internal_object_id);
    let collection_id = local_object_replies(&object_id);
    let first_page_id = format!("{}?page=true", collection_id);
    if query_params.page.is_none() {
        let collection = OrderedCollection::new(
            collection_id,
            Some(first_page_id),
            None,
            false,
        );
        let response = HttpResponse::Ok()
            .content_type(AP_MEDIA_TYPE)
            .json(collection);
        return Ok(response);
    };
    let mut replies: Vec<_> = posts.into_iter()
        .filter(|post| post.in_reply_to_id == Some(internal_object_id))
        .take(OrderedCollectionPage::DEFAULT_SIZE.into())
        .collect();
    add_related_posts(db_client, replies.iter_mut().collect()).await?;
    let objects = replies.iter().map(|post| {
        let object_id = post_object_id(&instance.url(), post);
        serde_json::to_value(object_id)
            .expect("string should be serializable")
    }).collect();
    let collection_page = OrderedCollectionPage::new(
        first_page_id,
        objects,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection_page);
    Ok(response)
}

#[get("/objects/emojis/{emoji_name}")]
pub async fn emoji_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    emoji_name: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let db_emoji = get_local_emoji_by_name(
        db_client,
        &emoji_name,
    ).await?;
    let object = build_emoji(
        &config.instance().url(),
        &db_emoji,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(object);
    Ok(response)
}

#[get("/collections/tags/{tag_name}")]
pub async fn tag_view(
    config: web::Data<Config>,
    tag_name: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let page_url = get_tag_page_url(&config.instance_url(), &tag_name);
    let response = HttpResponse::Found()
        .append_header((http_header::LOCATION, page_url))
        .finish();
    Ok(response)
}
