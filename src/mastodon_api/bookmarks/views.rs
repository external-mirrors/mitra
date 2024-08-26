use actix_web::{
    dev::ConnectionInfo,
    http::Uri,
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_models::{
    bookmarks::queries::get_bookmarked_posts,
    database::{get_database_client, DatabaseConnectionPool},
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
    statuses::helpers::get_paginated_status_list,
    timelines::types::TimelineQueryParams,
};

/// https://docs.joinmastodon.org/methods/bookmarks/
#[get("")]
async fn bookmark_list_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let posts = get_bookmarked_posts(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let response = get_paginated_status_list(
        db_client,
        &base_url,
        &instance_url,
        &request_uri,
        Some(&current_user),
        posts,
        &query_params.limit,
    ).await?;
    Ok(response)
}

pub fn bookmark_api_scope() -> Scope {
    web::scope("/v1/bookmarks")
        .service(bookmark_list_view)
}
