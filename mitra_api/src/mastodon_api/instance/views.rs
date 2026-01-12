use actix_web::{
    dev::ConnectionInfo,
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_adapters::dynamic_config::get_dynamic_config;
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    filter_rules::{
        queries::get_filter_rules,
        types::FilterAction,
    },
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
use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
    media_server::ClientMediaServer,
};

use super::types::{
    DomainBlock,
    InstanceInfo,
    InstanceInfoV2,
};

// https://docs.joinmastodon.org/methods/instance/#v1
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
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance = InstanceInfo::create(
        config.as_ref(),
        dynamic_config,
        &media_server,
        maybe_admin,
        user_count,
        post_count,
        peer_count,
    );
    Ok(HttpResponse::Ok().json(instance))
}

// https://docs.joinmastodon.org/methods/instance/#peers
#[get("/peers")]
async fn instance_peers_view(
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let peers = get_peers(db_client).await?;
    Ok(HttpResponse::Ok().json(peers))
}

// https://docs.joinmastodon.org/methods/instance/#domain_blocks
#[get("/domain_blocks")]
async fn domain_blocks_view(
    maybe_auth: Option<BearerAuth>,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let dynamic_config = get_dynamic_config(db_client).await?;
    if !dynamic_config.filter_blocklist_public {
        let auth = maybe_auth
            .ok_or(MastodonError::AuthError("authentication required"))?;
        get_current_user(db_client, auth.token()).await?;
    };
    let filter_rules = get_filter_rules(db_client).await?;
    let domain_blocks: Vec<_> = filter_rules
        .into_iter()
        .filter(|rule| matches!(
            rule.filter_action,
            FilterAction::Reject | FilterAction::RejectIncoming,
        ))
        // Stop when reverse rule is encountered
        .take_while(|rule| !rule.is_reversed)
        .map(|rule| DomainBlock::new(&rule.target))
        .collect();
    Ok(HttpResponse::Ok().json(domain_blocks))
}

pub fn instance_api_v1_scope() -> Scope {
    web::scope("/v1/instance")
        .service(instance_view)
        .service(instance_peers_view)
        .service(domain_blocks_view)
}

#[get("")]
pub async fn instance_v2_view(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let dynamic_config = get_dynamic_config(db_client).await?;
    let maybe_admin = if config.instance_staff_public {
        get_admin_user(db_client).await?
    } else {
        None
    };
    let user_count_active_month = get_active_user_count(
        db_client,
        days_before_now(28), // 4 weeks
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance = InstanceInfoV2::create(
        config.as_ref(),
        dynamic_config,
        &media_server,
        maybe_admin,
        user_count_active_month,
    );
    Ok(HttpResponse::Ok().json(instance))
}

pub fn instance_api_v2_scope() -> Scope {
    web::scope("/v2/instance")
        .service(instance_v2_view)
}
