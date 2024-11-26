/// https://docs.joinmastodon.org/methods/timelines/
use actix_web::{
    dev::ConnectionInfo,
    get,
    http::Uri,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_adapters::dynamic_config::get_dynamic_config;
use mitra_config::Config;
use mitra_models::{
    custom_feeds::queries::get_custom_feed,
    database::{get_database_client, DatabaseConnectionPool},
    posts::queries::{
        get_custom_feed_timeline,
        get_direct_timeline,
        get_home_timeline,
        get_posts_by_tag,
        get_public_timeline,
    },
    users::types::Permission,
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
    media_server::ClientMediaServer,
    statuses::helpers::get_paginated_status_list,
};
use super::types::{
    PublicTimelineQueryParams,
    TimelineQueryParams,
};

#[get("/home")]
async fn home_timeline(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let posts = get_home_timeline(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance_url = config.instance().url();
    let response = get_paginated_status_list(
        db_client,
        &base_url,
        &instance_url,
        &media_server,
        &request_uri,
        Some(&current_user),
        posts,
        &query_params.limit,
    ).await?;
    Ok(response)
}

/// Public timelines (local and known network)
#[get("/public")]
async fn public_timeline(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<PublicTimelineQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => {
            let current_user = get_current_user(db_client, auth.token()).await?;
            let dynamic_config = get_dynamic_config(db_client).await?;
            if dynamic_config.federated_timeline_restricted &&
                !query_params.local &&
                !current_user.role.has_permission(Permission::DeleteAnyPost)
            {
                return Err(MastodonError::PermissionError);
            };
            Some(current_user)
        },
        None => {
            // Show local timeline to guests only if enabled in config.
            // Never show TWKN to guests.
            if !config.instance_timeline_public || !query_params.local {
                return Err(MastodonError::AuthError("authentication required"));
            };
            None
        },
    };
    let posts = get_public_timeline(
        db_client,
        maybe_current_user.as_ref().map(|user| user.id),
        query_params.local,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance_url = config.instance().url();
    let response = get_paginated_status_list(
        db_client,
        &base_url,
        &instance_url,
        &media_server,
        &request_uri,
        maybe_current_user.as_ref(),
        posts,
        &query_params.limit,
    ).await?;
    Ok(response)
}

#[get("/direct")]
async fn direct_timeline(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let posts = get_direct_timeline(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance_url = config.instance().url();
    let response = get_paginated_status_list(
        db_client,
        &base_url,
        &instance_url,
        &media_server,
        &request_uri,
        Some(&current_user),
        posts,
        &query_params.limit,
    ).await?;
    Ok(response)
}

#[get("/tag/{hashtag}")]
async fn hashtag_timeline(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    hashtag: web::Path<String>,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let posts = get_posts_by_tag(
        db_client,
        &hashtag,
        maybe_current_user.as_ref().map(|user| user.id),
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance_url = config.instance().url();
    let response = get_paginated_status_list(
        db_client,
        &base_url,
        &instance_url,
        &media_server,
        &request_uri,
        maybe_current_user.as_ref(),
        posts,
        &query_params.limit,
    ).await?;
    Ok(response)
}

/// https://docs.joinmastodon.org/methods/timelines/#list
#[get("/list/{list_id}")]
async fn list_timeline(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    list_id: web::Path<i32>,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feed = get_custom_feed(
        db_client,
        *list_id,
        current_user.id,
    ).await?;
    let posts = get_custom_feed_timeline(
        db_client,
        feed.id,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance_url = config.instance().url();
    let response = get_paginated_status_list(
        db_client,
        &base_url,
        &instance_url,
        &media_server,
        &request_uri,
        Some(&current_user),
        posts,
        &query_params.limit,
    ).await?;
    Ok(response)
}

pub fn timeline_api_scope() -> Scope {
    web::scope("/v1/timelines")
        .service(home_timeline)
        .service(public_timeline)
        .service(direct_timeline)
        .service(hashtag_timeline)
        .service(list_timeline)
}
