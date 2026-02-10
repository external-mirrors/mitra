// https://docs.joinmastodon.org/methods/notifications/
use actix_web::{
    dev::ConnectionInfo,
    get,
    http::Uri,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    notifications::queries::{
        delete_notifications,
        get_notifications,
    },
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
    media_server::ClientMediaServer,
    pagination::{get_last_item, get_paginated_response},
};
use super::types::{
    Notification,
    NotificationQueryParams,
    NotificationPolicy,
};

// https://docs.joinmastodon.org/methods/notifications/#get
#[get("")]
async fn get_notifications_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<NotificationQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance = config.instance();
    let notifications: Vec<Notification> = get_notifications(
        db_client,
        current_user.id,
        query_params.min_id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?
        .into_iter()
        .map(|item| Notification::from_db(
            instance.uri_str(),
            &media_server,
            item,
        ))
        .collect();
    let maybe_last_id = get_last_item(&notifications, &query_params.limit)
        .map(|item| item.id.clone());
    let response = get_paginated_response(
        &base_url,
        &request_uri,
        notifications,
        maybe_last_id,
    );
    Ok(response)
}

// https://docs.joinmastodon.org/methods/notifications/#clear
#[post("/clear")]
async fn clear_notifications_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    delete_notifications(db_client, current_user.id).await?;
    let empty = serde_json::json!({});
    Ok(HttpResponse::Ok().json(empty))
}

// https://docs.joinmastodon.org/methods/notifications/#get-policy
#[get("/policy")]
async fn notification_policy_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let policy = NotificationPolicy::from_user(&current_user);
    Ok(HttpResponse::Ok().json(policy))
}

pub fn notification_api_v1_scope() -> Scope {
    web::scope("/v1/notifications")
        .service(get_notifications_view)
        .service(clear_notifications_view)
}

pub fn notification_api_v2_scope() -> Scope {
    web::scope("/v2/notifications")
        .service(notification_policy_view)
}
