use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    post,
    web,
    http::header as http_header,
    http::Uri,
    HttpRequest,
    HttpResponse,
    Scope,
};
use apx_core::{
    ap_url::with_ap_prefix,
    caip2::ChainId,
    hashlink::Hashlink,
    http_digest::ContentDigest,
    http_types::{header_map_adapter, method_adapter, uri_adapter},
};
use apx_sdk::{
    authentication::verify_portable_object,
    constants::{AP_MEDIA_TYPE, AP_PUBLIC},
    deserialization::object_to_id,
    http_server::is_activitypub_request,
    utils::get_core_type,
};
use log::Level;
use serde::Deserialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

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
    utils::parse_id_from_db,
};
use mitra_config::Config;
use mitra_models::{
    activitypub::queries::{
        create_activitypub_media,
        delete_activitypub_media,
        get_activitypub_media_by_digest,
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
        get_portable_user_by_id,
        get_portable_user_by_inbox_id,
        get_portable_user_by_outbox_id,
        get_user_by_name,
        get_user_by_name_with_pool,
    },
};
use mitra_services::media::{MediaServer, MediaStorage};
use mitra_validators::errors::ValidationError;

use crate::{
    errors::HttpError,
    http::get_request_full_uri,
    web_client::urls::{
        get_post_page_url,
        get_profile_page_url,
        get_subscription_page_url,
        get_tag_page_url,
    },
};

use super::{
    receiver::{receive_activity, InboxError},
    types::{
        GatewayMetadata,
        PortableActorKeys,
        PortableMedia,
    },
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
    let instance = config.instance();
    if !is_activitypub_request(&header_map_adapter(request.headers())) {
        let page_url = get_profile_page_url(
            instance.uri_str(),
            &user.profile.username,
        );
        let response = HttpResponse::Found()
            .append_header((http_header::LOCATION, page_url))
            .finish();
        return Ok(response);
    };
    let authority = Authority::server(instance.uri());
    let media_server = MediaServer::new(&config);
    let actor = build_local_actor(
        instance.uri(),
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
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    username: web::Path<String>,
    request: HttpRequest,
    request_body: web::Bytes,
) -> Result<HttpResponse, HttpError> {
    if !config.federation.enabled {
        return Err(HttpError::PermissionError);
    };
    let request_full_uri = get_request_full_uri(&connection_info, request.uri());
    let activity: JsonValue = serde_json::from_slice(&request_body)
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    log::info!("received in {}: {}", request.uri().path(), activity_type);
    log::debug!("activity: {activity}");

    let activity_digest = ContentDigest::new(&request_body);
    drop(request_body);

    let recipient = get_user_by_name_with_pool(&db_pool, &username).await?;
    let recipient_id = local_actor_id(
        config.instance().uri_str(),
        &recipient.profile.username,
    );
    receive_activity(
        &config,
        &db_pool,
        &request,
        &request_full_uri,
        &activity,
        activity_digest,
        &recipient_id,
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
    let actor_id = local_actor_id(instance.uri_str(), &username);
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
            let activity = build_announce(instance.uri_str(), post);
            serde_json::to_value(activity)
                .expect("activity should be serializable")
        } else {
            let activity = build_create_note(
                instance.uri(),
                &media_server,
                post,
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
    let actor_id = local_actor_id(config.instance().uri_str(), &username);
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
    let actor_id = local_actor_id(config.instance().uri_str(), &username);
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
    let actor_id = local_actor_id(config.instance().uri_str(), &username);
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
    let actor_id = local_actor_id(instance.uri_str(), &username);
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
    let authority = Authority::server(instance.uri());
    let media_server = MediaServer::new(&config);
    let objects = posts.iter().map(|post| {
        let note = build_note(
            instance.uri(),
            &authority,
            &media_server,
            post,
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
        .ok_or(HttpError::NotFound("proposal"))?;
    let payment_info = match payment_option {
        PaymentOption::MoneroSubscription(payment_info)
            if is_activitypub_request(&header_map_adapter(request.headers())) => payment_info,
        PaymentOption::MoneroSubscription(_) => {
            let page_url = get_subscription_page_url(
                config.instance().uri_str(),
                &user.profile.username,
            );
            let response = HttpResponse::Found()
                .append_header((http_header::LOCATION, page_url))
                .finish();
            return Ok(response);
        },
        _ => unreachable!("local payment option should not be link"),
    };
    let proposal = build_proposal(
        config.instance().uri_str(),
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
        .map_err(HttpError::from_internal)?;
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
        return Err(HttpError::NotFound("post"));
    };
    if !is_activitypub_request(&header_map_adapter(request.headers())) {
        let page_url = get_post_page_url(instance.uri_str(), post.id);
        let response = HttpResponse::Found()
            .append_header((http_header::LOCATION, page_url))
            .finish();
        return Ok(response);
    };
    add_related_posts(db_client, vec![&mut post]).await?;
    let authority = Authority::from(&instance);
    let media_server = MediaServer::new(&config);
    let object = build_note(
        instance.uri(),
        &authority,
        &media_server,
        &post,
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
        return Err(HttpError::NotFound("post"));
    };
    let instance = config.instance();
    let object_id = local_object_id(instance.uri_str(), internal_object_id);
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
        let object_id = compatible_post_object_id(instance.uri_str(), post);
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
        config.instance().uri_str(),
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
    let page_url = get_tag_page_url(config.instance().uri_str(), &tag_name);
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
        return Err(HttpError::NotFound("conversation"));
    };
    let instance = config.instance();
    let collection_id =
        local_conversation_collection(instance.uri_str(), *conversation_id);
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
            let object_id = compatible_post_object_id(instance.uri_str(), post);
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
        config.instance().uri(),
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

#[get("")]
async fn apgateway_metadata_view(
    config: web::Data<Config>,
) -> HttpResponse {
    let metadata = GatewayMetadata {
        upload_media: format!(
            "{}/.well-known/apgateway-media",
            config.instance().uri(),
        ),
    };
    HttpResponse::Ok().json(metadata)
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
            other_error => HttpError::from_internal(other_error),
        }
    })?;
    log::warn!("created portable account {}", user);
    let keys = PortableActorKeys::new(user);
    Ok(HttpResponse::Created().json(keys))
}

#[get("/{url:.+}")]
async fn apgateway_view(
    db_pool: web::Data<DatabaseConnectionPool>,
    did_url: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let ap_uri = with_ap_prefix(&did_url);
    let object_value = match get_actor(db_client, &ap_uri).await {
        Ok(actor_value) => actor_value,
        Err(DatabaseError::NotFound(_)) => {
            get_object_as_target(
                db_client,
                &ap_uri,
                AP_PUBLIC,
            ).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // Serve object only if its owner has local account
    let core_type = get_core_type(&object_value);
    let owner_id = get_owner(&object_value, core_type)
        .map_err(|_| HttpError::NotFound("object"))?;
    let canonical_owner_id = canonicalize_id(&owner_id)?;
    let owner = get_remote_profile_by_actor_id(
        db_client,
        &canonical_owner_id.to_string(),
    ).await?;
    if !owner.has_account() {
        return Err(HttpError::NotFound("object"));
    };
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(object_value);
    Ok(response)
}

#[post("/{url:.+}/inbox")]
async fn apgateway_inbox_push_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    request_path: Uri,
    request_body: web::Bytes,
) -> Result<HttpResponse, HttpError> {
    if !config.federation.enabled {
        return Err(HttpError::PermissionError);
    };
    let request_full_uri = get_request_full_uri(&connection_info, request.uri());
    let activity: JsonValue = serde_json::from_slice(&request_body)
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    log::info!("received in {}: {}", request.uri().path(), activity_type);

    let activity_digest = ContentDigest::new(&request_body);
    drop(request_body);

    let collection_id = format!(
        "{}{}",
        config.instance().uri(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let recipient = {
        let db_client = &**get_database_client(&db_pool).await?;
        get_portable_user_by_inbox_id(
            db_client,
            &canonical_collection_id.to_string(),
        ).await?
    };
    let recipient_id = recipient.profile.expect_remote_actor_id();
    receive_activity(
        &config,
        &db_pool,
        &request,
        &request_full_uri,
        &activity,
        activity_digest,
        recipient_id,
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
#[get("/{url:.+}/inbox")]
async fn apgateway_inbox_pull_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_path: Uri,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let request_full_uri = get_request_full_uri(&connection_info, request.uri());
    let (_, signer) = verify_signed_request(
        &config,
        &db_pool,
        method_adapter(request.method()),
        uri_adapter(&request_full_uri),
        header_map_adapter(request.headers()),
        None, // GET request has no content
        true, // don't fetch actor
    ).await.map_err(|error| {
        log::warn!("C2S authentication error (GET {request_path}): {error}");
        HttpError::AuthError("invalid signature")
    })?;
    let canonical_signer_id = parse_id_from_db(signer.expect_remote_actor_id())?;
    let collection_id = format!(
        "{}{}",
        config.instance().uri(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let db_client = &**get_database_client(&db_pool).await?;
    let collection_owner = get_portable_user_by_inbox_id(
        db_client,
        &canonical_collection_id.to_string(),
    ).await?;
    let canonical_owner_id =
        parse_id_from_db(collection_owner.profile.expect_remote_actor_id())?;
    if canonical_owner_id != canonical_signer_id {
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

#[post("/{url:.+}/outbox")]
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
        instance.uri(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let collection_owner = get_portable_user_by_outbox_id(
        db_client,
        &canonical_collection_id.to_string(),
    ).await?;
    let canonical_owner_id =
        parse_id_from_db(collection_owner.profile.expect_remote_actor_id())?;
    // Verify activity
    verify_portable_object(&activity).map_err(|error| {
        log::warn!("C2S authentication error (POST {request_path}): {error}");
        HttpError::PermissionError
    })?;
    let activity_actor = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid 'actor' property"))?;
    let canonical_actor_id = canonicalize_id(&activity_actor)?;
    if canonical_actor_id != canonical_owner_id {
        return Err(HttpError::PermissionError);
    };
    validate_public_keys(
        &config.instance(),
        Some(&collection_owner),
        &activity,
    )?;
    IncomingActivityJobData::new(
        &activity,
        None, // no inbox
        true, // activity has been authenticated
    )
        .into_job(db_client, 0)
        .await?;
    Ok(HttpResponse::Accepted().finish())
}

#[get("/{url:.+}/outbox")]
async fn apgateway_outbox_pull_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_path: Uri,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let request_full_uri = get_request_full_uri(&connection_info, request.uri());
    let (_, signer) = verify_signed_request(
        &config,
        &db_pool,
        method_adapter(request.method()),
        uri_adapter(&request_full_uri),
        header_map_adapter(request.headers()),
        None, // GET request has no content
        true, // don't fetch actor
    ).await.map_err(|error| {
        log::warn!("C2S authentication error (GET {request_path}): {error}");
        HttpError::AuthError("invalid signature")
    })?;
    let canonical_signer_id = parse_id_from_db(signer.expect_remote_actor_id())?;
    let collection_id = format!(
        "{}{}",
        config.instance().uri(),
        request_path,
    );
    let canonical_collection_id = canonicalize_id(&collection_id)?;
    let db_client = &**get_database_client(&db_pool).await?;
    let collection_owner = get_portable_user_by_outbox_id(
        db_client,
        &canonical_collection_id.to_string(),
    ).await?;
    let canonical_owner_id =
        parse_id_from_db(collection_owner.profile.expect_remote_actor_id())?;
    if canonical_owner_id != canonical_signer_id {
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
        .service(apgateway_metadata_view)
        .service(apgateway_create_actor_view)
        // Inbox and outbox services go before generic gateway service
        .service(apgateway_inbox_push_view)
        .service(apgateway_inbox_pull_view)
        .service(apgateway_outbox_push_view)
        .service(apgateway_outbox_pull_view)
        .service(apgateway_view)
}

#[post("")]
async fn apgateway_media_upload_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    request_body: web::Bytes,
) -> Result<HttpResponse, HttpError> {
    let request_full_uri =
        get_request_full_uri(&connection_info, request.uri());
    let body_digest = ContentDigest::new(&request_body);
    let (_, signer) = verify_signed_request(
        &config,
        &db_pool,
        method_adapter(request.method()),
        uri_adapter(&request_full_uri),
        header_map_adapter(request.headers()),
        Some(body_digest),
        true, // don't fetch actor
    ).await.map_err(|error| {
        log::warn!("C2S authentication error (POST {request_full_uri}): {error}");
        HttpError::AuthError("invalid signature")
    })?;
    let db_client = &**get_database_client(&db_pool).await?;
    let signer = match get_portable_user_by_id(
        db_client,
        signer.id,
    ).await {
        Ok(signer) => signer,
        Err(DatabaseError::NotFound(_)) => {
            // Only local portable users can upload media
            return Err(HttpError::PermissionError);
        },
        Err(other_error) => return Err(other_error.into()),
    };

    let storage = MediaStorage::new(&config);
    const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";
    let media_type = request.headers()
        .get(http_header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or(APPLICATION_OCTET_STREAM);
    let file_data = request_body.to_vec();
    if file_data.len() > config.limits.media.file_size_limit {
        return Err(HttpError::PayloadTooLarge);
    };
    if !config.limits.media.supported_media_types().contains(&media_type) {
        return Err(ValidationError("invalid media type").into());
    };
    let file_info = storage.save_file(file_data, media_type)
        .map_err(HttpError::from_internal)?;
    create_activitypub_media(
        db_client,
        signer.id,
        file_info.clone(),
    ).await?;

    let hashlink = Hashlink::new(file_info.digest);
    let media_data = PortableMedia {
        url: hashlink.to_string(),
    };
    let response = HttpResponse::Created()
        .content_type(AP_MEDIA_TYPE)
        .json(media_data);
    Ok(response)
}

#[get("/{hashlink:.+}")]
async fn apgateway_media_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    hashlink: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let digest = Hashlink::parse(&hashlink)
        .map_err(|_| ValidationError("invalid hashlink"))?
        .digest();
    // TODO: FEP-ef61: check object ID, HTTP signature & permission
    let db_client = &**get_database_client(&db_pool).await?;
    let file_info = get_activitypub_media_by_digest(db_client, digest).await?;
    let media_server = MediaServer::new(&config);
    let media_url = media_server.url_for(&file_info.file_name);
    let response = HttpResponse::Found()
        .append_header((http_header::LOCATION, media_url))
        .finish();
    Ok(response)
}

#[delete("/{hashlink:.+}")]
async fn apgateway_media_delete_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    hashlink: web::Path<String>,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let request_full_uri =
        get_request_full_uri(&connection_info, request.uri());
    let (_, signer) = verify_signed_request(
        &config,
        &db_pool,
        method_adapter(request.method()),
        uri_adapter(&request_full_uri),
        header_map_adapter(request.headers()),
        None,
        true, // don't fetch actor
    ).await.map_err(|error| {
        log::warn!("C2S authentication error (DELETE {request_full_uri}): {error}");
        HttpError::AuthError("invalid signature")
    })?;
    let db_client = &**get_database_client(&db_pool).await?;
    let signer = match get_portable_user_by_id(
        db_client,
        signer.id,
    ).await {
        Ok(signer) => signer,
        Err(DatabaseError::NotFound(_)) => {
            // Only local portable users can delete media
            return Err(HttpError::PermissionError);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    let digest = Hashlink::parse(&hashlink)
        .map_err(|_| ValidationError("invalid hashlink"))?
        .digest();
    delete_activitypub_media(db_client, signer.id, digest).await?;
    let response = HttpResponse::NoContent().finish();
    Ok(response)
}

pub fn media_gateway_scope(gateway_enabled: bool) -> Scope {
    let scope = web::scope("/.well-known/apgateway-media");
    if !gateway_enabled {
        return scope;
    };
    scope
        .service(apgateway_media_view)
        .service(apgateway_media_upload_view)
        .service(apgateway_media_delete_view)
}
