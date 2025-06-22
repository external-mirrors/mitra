/// https://docs.joinmastodon.org/methods/instance/directory/
use actix_web::{
    dev::ConnectionInfo,
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    profiles::queries::get_profiles_paginated,
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account,
    auth::get_current_user,
    errors::MastodonError,
    media_server::ClientMediaServer,
};
use super::types::DirectoryQueryParams;

#[get("")]
async fn profile_directory(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<DirectoryQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let profiles = get_profiles_paginated(
        db_client,
        query_params.local,
        query_params.db_order(),
        query_params.offset,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles
        .into_iter()
        .map(|profile| Account::from_profile(
            &instance_url,
            &media_server,
            profile,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

pub fn directory_api_scope() -> Scope {
    web::scope("/v1/directory")
        .service(profile_directory)
}
