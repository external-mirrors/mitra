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
    database::{get_database_client, DatabaseConnectionPool},
    relationships::queries::get_mutes_paginated,
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account,
    auth::get_current_user,
    errors::MastodonError,
    pagination::{get_last_item, get_paginated_response},
};

use super::types::MuteListQueryParams;

/// https://docs.joinmastodon.org/methods/mutes/#get
#[get("")]
async fn mute_list_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<MuteListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profiles = get_mutes_paginated(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let maybe_last_id = get_last_item(&profiles, &query_params.limit)
        .map(|item| item.related_id);
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|item| Account::from_profile(
            &base_url,
            &instance_url,
            item.profile,
        ))
        .collect();
    let response = get_paginated_response(
        &base_url,
        &request_uri,
        accounts,
        maybe_last_id,
    );
    Ok(response)
}

pub fn mute_api_scope() -> Scope {
    web::scope("/v1/mutes")
        .service(mute_list_view)
}
