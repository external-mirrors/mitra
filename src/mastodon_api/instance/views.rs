use actix_web::{
    dev::ConnectionInfo,
    get,
    web,
    HttpResponse,
    Scope,
};

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    instances::queries::{get_peers, get_peer_count},
    posts::queries::get_post_count,
    users::queries::{get_admin_user, get_user_count},
};
use mitra_services::ethereum::contracts::ContractSet;

use crate::adapters::dynamic_config::get_dynamic_config;
use crate::http::get_request_base_url;
use crate::mastodon_api::errors::MastodonError;

use super::types::InstanceInfo;

/// https://docs.joinmastodon.org/methods/instance/#v1
#[get("")]
async fn instance_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    maybe_ethereum_contracts: web::Data<Option<ContractSet>>,
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
        maybe_ethereum_contracts.as_ref().as_ref(),
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

pub fn instance_api_scope() -> Scope {
    web::scope("/api/v1/instance")
        .service(instance_view)
        .service(instance_peers_view)
}
