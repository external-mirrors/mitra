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
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    reactions::queries::get_liked_posts,
};

use crate::{
    http::get_request_base_url,
    mastodon_api::{
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
        pagination::{get_last_item, get_paginated_response},
        statuses::helpers::build_status_list,
    },
};

use super::types::FavListQueryParams;

// https://docs.joinmastodon.org/methods/favourites/#get
#[get("")]
async fn get_favourites_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<FavListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let liked_posts = get_liked_posts(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance = config.instance();
    let maybe_last_id = get_last_item(&liked_posts, &query_params.limit)
        .map(|liked_post| liked_post.reaction_id);
    let posts = liked_posts.into_iter()
        .map(|liked_post| liked_post.post)
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

pub fn favourite_api_scope() -> Scope {
    web::scope("/v1/favourites")
        .service(get_favourites_view)
}
