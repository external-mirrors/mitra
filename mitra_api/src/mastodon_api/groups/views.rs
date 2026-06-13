use actix_web::{
    dev::ConnectionInfo,
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_activitypub::authority::Authority;
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    groups::queries::get_followed_groups,
};

use crate::{
    http::{
        get_request_base_url,
    },
    mastodon_api::{
        accounts::types::Account,
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
    },
};

use super::types::GroupListQueryParams;

#[get("/followed")]
async fn get_followed_groups_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<GroupListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let groups = get_followed_groups(
        db_client,
        current_user.id,
        query_params.offset,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let authority = Authority::from(&config.instance());
    let media_server = ClientMediaServer::new(&config, &base_url);
    let accounts: Vec<Account> = groups.into_iter()
        .map(|profile| Account::from_profile(
            &authority,
            &media_server,
            profile,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

pub fn group_api_scope() -> Scope {
    web::scope("/v1/groups")
        .service(get_followed_groups_view)
}
