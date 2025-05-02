use actix_web::{
    get,
    post,
    web,
    http::header as http_header,
    http::Uri,
    HttpRequest,
    HttpResponse,
    Scope,
};
use log::Level;
use serde::Deserialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use apx_core::{
    ap_url::with_ap_prefix,
    caip2::ChainId,
    http_digest::ContentDigest,
    http_types::{method_adapter, uri_adapter},
};
use apx_sdk::{
    authentication::verify_portable_object,
    constants::{AP_MEDIA_TYPE, AP_PUBLIC},
    deserialization::object_to_id,
    http_server::is_activitypub_request,
    utils::get_core_type,
};
use mitra_activitypub::{
    actors::builders::{
        build_instance_actor,
        build_local_actor,
    },
    authentication::verify_signed_request,
    authority::Authority,
    builders::{
        announce::build_announce,
        collection::{
            OrderedCollection,
            OrderedCollectionPage,
        },
        create_note::build_create_note,
        emoji::build_emoji,
        note::build_note,
        proposal::build_proposal,
    },
    errors::HandlerError,
    forwarder::validate_public_keys,
    identifiers::{
        canonicalize_id,
        compatible_post_object_id,
        local_actor_id,
        local_conversation_collection,
        local_object_id,
        local_object_replies,
        LocalActorCollection,
    },
    importers::register_portable_actor,
    ownership::get_owner,
    queues::IncomingActivityJobData,
};
use mitra_config::Config;
use mitra_models::{
    activitypub::queries::{
        get_actor,
        get_collection_items,
        get_object_as_target,
    },
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    emojis::queries::get_local_emoji_by_name,
    posts::helpers::{
        add_related_posts,
        get_post_by_id_for_view,
    },
    posts::queries::{
        get_conversation_items,
        get_posts_by_author,
        get_thread,
    },
    profiles::{
        queries::get_remote_profile_by_actor_id,
        types::PaymentOption,
    },
    users::queries::{
        get_portable_user_by_actor_id,
        get_portable_user_by_inbox_id,
        get_portable_user_by_outbox_id,
        get_user_by_name,
    },
};
use mitra_services::media::MediaServer;
use mitra_validators::errors::ValidationError;

use crate::{
    errors::HttpError,
    http::actix_header_map_adapter,
    web_client::urls::{
        get_post_page_url,
        get_profile_page_url,
        get_subscription_page_url,
        get_tag_page_url,
    },
};

use super::{
    receiver::{receive_activity, InboxError},
    types::PortableActorKeys,
};

#[get("")]
async fn actor_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    username: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    if !is_activitypub_request(&actix_header_map_adapter(request.headers())) {
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
        false,
    );
    let media_server = MediaServer::new(&config);
    let actor = build_local_actor(
        &config.instance_url(),
        &authority,
        &media_server,
        &user,
    )?;
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(actor);
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
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    log::info!("received in {}: {}", request.uri().path(), activity_type);
    log::debug!("activity: {activity}");

    let activity_digest = ContentDigest::new(&request_body);
    drop(request_body);

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
            let log_level = match error {
                InboxError::DatabaseError(_) => Level::Error,
                _ => Level::Warn,
            };
            log::log!(
                log_level,
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
        user.id,
        None, // include only public posts
        true, // include replies
        true, // include reposts
        false, // not only pinned
        false, // not only media
        None,
        OrderedCollectionPage::DEFAULT_SIZE,
    ).await?;
    add_related_posts(db_client, posts.iter_mut().collect()).await?;
    let media_server = MediaServer::new(&config);
    let activities = posts.iter().map(|post| {
        if post.repost_of_id.is_some() {
            let activity = build_announce(&instance.url(), post);
            serde_json::to_value(activity)
                .expect("activity should be serializable")
        } else {
            let activity = build_create_note(
                &instance.hostname(),
                &instance.url(),
                &media_server,
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
async fn outbox_client_to_server() -> HttpResponse {
    HttpResponse::MethodNotAllowed().finish()
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
        user.id,
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
    let media_server = MediaServer::new(&config);
    let objects = posts.iter().map(|post| {
        let note = build_note(
            &instance.hostname(),
            &instance.url(),
            &authority,
            &media_server,
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
            if is_activitypub_request(&actix_header_map_adapter(request.headers())) => payment_info,
        PaymentOption::MoneroSubscription(_) => {
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
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let internal_object_id = internal_object_id.into_inner();
    let instance = config.instance();
    // Try to find local post by ID,
    // return 404 if not found, or not public, or it is a repost
    let mut post = get_post_by_id_for_view(
        db_client,
        None,
        internal_object_id,
    ).await?;
    if !post.is_local() {
        return Err(HttpError::NotFoundError("post"));
    };
    if !is_activitypub_request(&actix_header_map_adapter(request.headers())) {
        let page_url = get_post_page_url(&instance.url(), post.id);
        let response = HttpResponse::Found()
            .append_header((http_header::LOCATION, page_url))
            .finish();
        return Ok(response);
    };
    add_related_posts(db_client, vec![&mut post]).await?;
    let authority = Authority::from(&instance);
    let media_server = MediaServer::new(&config);
    let object = build_note(
        &instance.hostname(),
        &instance.url(),
        &authority,
        &media_server,
        &post,
        config.federation.fep_e232_enabled,
        true, // with_context
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(object);
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
    let posts = get_thread(db_client, internal_object_id, None).await?;
    let post = posts.iter().find(|post| post.id == internal_object_id)
        .expect("get_thread return value should contain target post");
    // Visibility check is done in get_thread
    if !post.is_local() {
        return Err(HttpError::NotFoundError("post"));
    };
    let instance = config.instance();
    let object_id = local_object_id(&instance.url(), internal_object_id);
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
    let replies: Vec<_> = posts.into_iter()
        .filter(|post| post.in_reply_to_id == Some(internal_object_id))
        .take(OrderedCollectionPage::DEFAULT_SIZE.into())
        .collect();
    let objects = replies.iter().map(|post| {
        let object_id = compatible_post_object_id(&instance.url(), post);
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
    let media_server = MediaServer::new(&config);
    let object = build_emoji(
        &config.instance().url(),
        &media_server,
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

#[get("/collections/conversations/{conversation_id}")]
pub async fn conversation_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    conversation_id: web::Path<Uuid>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let (root, posts) = get_conversation_items(
        db_client,
        *conversation_id,
        None, // viewing as guest
    ).await?;
    if !root.is_local() {
        return Err(HttpError::NotFoundError("conversation"));
    };
    let instance = config.instance();
    let collection_id =
        local_conversation_collection(&instance.url(), *conversation_id);
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
    let objects = posts.iter()
        .take(OrderedCollectionPage::DEFAULT_SIZE.into())
        .map(|post| {
            let object_id = compatible_post_object_id(&instance.url(), post);
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

#[get("/activities/{tail:.*}")]
pub async fn activity_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_path: Uri,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let activity_id = format!(
        "{}{}",
        config.instance_url(),
        request_path,
    );
    let activity = get_object_as_target(
        db_client,
        &activity_id,
        AP_PUBLIC,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(activity);
    Ok(response)
}

#[post("")]
async fn apgateway_create_actor_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    actor: web::Json<JsonValue>,
) -> Result<HttpResponse, HttpError> {
    let invite_code = request.headers()
        .get("X-Invite-Code")
        .and_then(|value| value.to_str().ok())
        .ok_or(ValidationError("invite code is required"))?;
    validate_public_keys(
        &config.instance(),
        None,
        &actor,
    )?;
    let db_client = &mut **get_database_client(&db_pool).await?;
    let user = register_portable_actor(
        &config,
        db_client,
        actor.into_inner(),
        invite_code,
    ).await.map_err(|error| {
        log::warn!("failed to register portable actor ({error})");
        match error {
            HandlerError::ValidationError(error) =>
                HttpError::ValidationError(error),
            HandlerError::DatabaseError(error) => error.into(),
            _ => HttpError::InternalError,
        }
    })?;
    log::warn!("created portable account {}", user);
    let keys = PortableActorKeys::new(user);
    Ok(HttpResponse::Created().json(keys))
}

#[get("/{url:.*}")]
async fn apgateway_view(
    db_pool: web::Data<DatabaseConnectionPool>,
    did_url: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let ap_url = with_ap_prefix(&did_url);
    let object_value = match get_actor(db_client, &ap_url).await {
        Ok(actor_value) => actor_value,
        Err(DatabaseError::NotFound(_)) => {
            get_object_as_target(
                db_client,
                &ap_url,
                AP_PUBLIC,
            ).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // Serve object only if its owner has local account
    let core_type = get_core_type(&object_value);
    let owner_id = get_owner(&object_value, core_type)
        .map_err(|_| HttpError::NotFoundError("object"))?;
    let canonical_owner_id = canonicalize_id(&owner_id)?;
    let owner = get_remote_profile_by_actor_id(
        db_client,
        &canonical_owner_id.to_string(),
    ).await?;
    if !owner.has_account() {
        return Err(HttpError::NotFoundError("object"));
    };
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(object_value);
    Ok(response)
}

#[post("/{url:.*}/inbox")]
async fn apgateway_inbox_push_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    request_path: Uri,
    request_body: web::Bytes,
) -> Result<HttpResponse, HttpError> {
    if !config.federation.enabled {
        return Err(HttpError::PermissionError);
    };
    let activity: JsonValue = serde_json::from_slice(&request_body)
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    log::info!("received in {}: {}", request.uri().path(), activity_type);

    let activity_digest = ContentDigest::new(&request_body);
    drop(request_body);

    let collection_id = format!(
        "{}{}",
        config.instance_url(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let db_client = &mut **get_database_client(&db_pool).await?;
    let _portable_user = get_portable_user_by_inbox_id(
        db_client,
        &canonical_collection_id.to_string(),
    ).await?;

    receive_activity(
        &config,
        db_client,
        &request,
        &activity,
        activity_digest,
    ).await
        .map_err(|error| {
            log::warn!(
                "failed to process activity ({}): {}",
                error,
                activity,
            );
            error
        })?;
    Ok(HttpResponse::Accepted().finish())
}

// TODO: FEP-EF61: how to detect collections?
// TODO: shared inbox?
#[get("/{url:.*}/inbox")]
async fn apgateway_inbox_pull_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_path: Uri,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let (signing_key_id, _) = verify_signed_request(
        &config,
        db_client,
        method_adapter(request.method()),
        uri_adapter(request.uri()),
        actix_header_map_adapter(request.headers()),
        None, // GET request has no content
        true, // don't fetch actor
        true, // only key
    ).await.map_err(|error| {
        log::warn!("C2S authentication error (GET {request_path}): {error}");
        HttpError::PermissionError
    })?;
    let collection_id = format!(
        "{}{}",
        config.instance_url(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let collection_owner = get_portable_user_by_inbox_id(
        db_client,
        &canonical_collection_id.to_string(),
    ).await?;
    let canonical_owner_id =
        canonicalize_id(collection_owner.profile.expect_remote_actor_id())?;
    if canonical_owner_id.origin() != signing_key_id.origin() {
        return Err(HttpError::PermissionError);
    };
    const LIMIT: u32 = 20;
    let items = get_collection_items(
        db_client,
        &canonical_collection_id.to_string(),
        LIMIT,
    ).await?;
    // TODO: FEP-EF61: collection or collection page?
    let collection_page = OrderedCollectionPage::new(
        collection_id,
        items,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection_page);
    Ok(response)
}

#[post("/{url:.*}/outbox")]
async fn apgateway_outbox_push_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_path: Uri,
    activity: web::Json<JsonValue>,
) -> Result<HttpResponse, HttpError> {
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    log::info!("received in {}: {}", request_path, activity_type);
    let db_client = &mut **get_database_client(&db_pool).await?;
    let instance = config.instance();
    // Find outbox owner
    let collection_id = format!(
        "{}{}",
        instance.url(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let collection_owner = match get_portable_user_by_outbox_id(
        db_client,
        &canonical_collection_id.to_string(),
    ).await {
        Ok(signer) => signer,
        Err(DatabaseError::NotFound(_)) => {
            // Only local portable users can post to outbox
            return Ok(HttpResponse::MethodNotAllowed().finish());
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // Verify activity
    verify_portable_object(&activity).map_err(|error| {
        log::warn!("C2S authentication error (POST {request_path}): {error}");
        HttpError::PermissionError
    })?;
    let activity_actor = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid 'actor' property"))?;
    let canonical_actor_id = canonicalize_id(&activity_actor)?;
    let signer = get_portable_user_by_actor_id(
        db_client,
        &canonical_actor_id.to_string(),
    ).await?;
    if signer.id != collection_owner.id {
        return Err(HttpError::PermissionError);
    };
    validate_public_keys(
        &config.instance(),
        Some(&collection_owner),
        &activity,
    )?;
    IncomingActivityJobData::new(&activity, true)
        .into_job(db_client, 0)
        .await?;
    Ok(HttpResponse::Accepted().finish())
}

#[get("/{url:.*}/outbox")]
async fn apgateway_outbox_pull_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_path: Uri,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let (signing_key_id, _) = verify_signed_request(
        &config,
        db_client,
        method_adapter(request.method()),
        uri_adapter(request.uri()),
        actix_header_map_adapter(request.headers()),
        None, // GET request has no content
        true, // don't fetch actor
        true, // only key
    ).await.map_err(|error| {
        log::warn!("C2S authentication error (GET {request_path}): {error}");
        HttpError::PermissionError
    })?;
    let collection_id = format!(
        "{}{}",
        config.instance_url(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let collection_owner = get_portable_user_by_outbox_id(
        db_client,
        &canonical_collection_id.to_string(),
    ).await?;
    let canonical_owner_id =
        canonicalize_id(collection_owner.profile.expect_remote_actor_id())?;
    if canonical_owner_id.origin() != signing_key_id.origin() {
        return Err(HttpError::PermissionError);
    };
    const LIMIT: u32 = 20;
    let items = get_collection_items(
        db_client,
        &canonical_collection_id.to_string(),
        LIMIT,
    ).await?;
    // TODO: FEP-EF61: collection or collection page?
    let collection_page = OrderedCollectionPage::new(
        collection_id,
        items,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection_page);
    Ok(response)
}

pub fn gateway_scope(gateway_enabled: bool) -> Scope {
    let scope = web::scope("/.well-known/apgateway");
    if !gateway_enabled {
        return scope;
    };
    scope
        .service(apgateway_create_actor_view)
        // Inbox and outbox services go before generic gateway service
        .service(apgateway_inbox_push_view)
        .service(apgateway_inbox_pull_view)
        .service(apgateway_outbox_push_view)
        .service(apgateway_outbox_pull_view)
        .service(apgateway_view)
}
