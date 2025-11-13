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
    media_server::ClientMediaServer,
    pagination::{get_last_item, get_paginated_response},
    statuses::helpers::build_status_list,
};

use super::types::BookmarkListQueryParams;

/// https://docs.joinmastodon.org/methods/bookmarks/
#[get("")]
async fn bookmark_list_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<BookmarkListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let bookmarks = get_bookmarked_posts(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance = config.instance();
    let maybe_last_id = get_last_item(&bookmarks, &query_params.limit)
        .map(|bookmark| bookmark.bookmark_id);
    let posts = bookmarks.into_iter()
        .map(|bookmark| bookmark.post)
        .collect();
    let statuses = build_status_list(
        db_client,
        instance.uri_str(),
        &media_server,
        Some(&current_user),
        posts,
    ).await?;
    let response = get_paginated_response(
        &base_url,
        &request_uri,
        statuses,
        maybe_last_id,
    );
    Ok(response)
}

pub fn bookmark_api_scope() -> Scope {
    web::scope("/v1/bookmarks")
        .service(bookmark_list_view)
}
