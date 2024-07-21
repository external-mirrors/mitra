use actix_web::{
    dev::ConnectionInfo,
    get,
    web,
    HttpResponse,
    Scope,
};

use mitra_adapters::dynamic_config::get_dynamic_config;
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    instances::queries::{get_peers, get_peer_count},
    posts::queries::get_post_count,
    users::queries::{
        get_active_user_count,
        get_admin_user,
        get_user_count,
    },
};
use mitra_utils::datetime::days_before_now;

use crate::http::get_request_base_url;
use crate::mastodon_api::errors::MastodonError;

use super::types::{InstanceInfo, InstanceInfoV2};

/// https://docs.joinmastodon.org/methods/instance/#v1
#[get("")]
async fn instance_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_admin = if config.instance_staff_public {
        get_admin_user(db_client).await?
    } else {
        None
    };
    let user_count = get_user_count(db_client).await?;
    let post_count = get_post_count(db_client, true).await?;
    let peer_count = get_peer_count(db_client).await?;
    let dynamic_config = get_dynamic_config(db_client).await?;
    let instance = InstanceInfo::create(
        &get_request_base_url(connection_info),
        config.as_ref(),
        dynamic_config,
        maybe_admin,
        user_count,
        post_count,
        peer_count,
    );
    Ok(HttpResponse::Ok().json(instance))
}

#[get("/peers")]
async fn instance_peers_view(
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let peers = get_peers(db_client).await?;
    Ok(HttpResponse::Ok().json(peers))
}

pub fn instance_api_v1_scope() -> Scope {
    web::scope("/v1/instance")
        .service(instance_view)
        .service(instance_peers_view)
}

#[get("")]
async fn instance_v2_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_admin = if config.instance_staff_public {
        get_admin_user(db_client).await?
    } else {
        None
    };
    let user_count_active_month = get_active_user_count(
        db_client,
        days_before_now(28), // 4 weeks
    ).await?;
    let instance = InstanceInfoV2::create(
        &get_request_base_url(connection_info),
        config.as_ref(),
        maybe_admin,
        user_count_active_month,
    );
    Ok(HttpResponse::Ok().json(instance))
}

pub fn instance_api_v2_scope() -> Scope {
    web::scope("/v2/instance")
        .service(instance_v2_view)
}
