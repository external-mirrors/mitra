use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::{
    adapters::users::delete_account,
    authority::Authority,
};
use mitra_config::Config;
use mitra_models::{
    accounts::{
        queries::{
            get_accounts_for_admin,
            get_managed_account_by_id,
        },
        types::Permission,
    },
    database::{get_database_client, DatabaseConnectionPool},
    profiles::queries::{delete_profile, get_profile_by_id},
};

use crate::{
    http::get_request_base_url,
    mastodon_api::{
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
    },
};

use super::types::AdminAccount;

// https://docs.joinmastodon.org/methods/admin/accounts/#v2
#[get("")]
async fn account_list_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::DeleteAnyProfile) {
        return Err(MastodonError::PermissionError);
    };
    let users = get_accounts_for_admin(db_client).await?;
    let authority = Authority::from(&config.instance());
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let accounts: Vec<AdminAccount> = users.into_iter()
        .map(|user| AdminAccount::from_db(
            &authority,
            &media_server,
            user,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

// https://docs.joinmastodon.org/methods/admin/accounts/#delete
#[delete("/{account_id}")]
async fn delete_account_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::DeleteAnyProfile) {
        return Err(MastodonError::PermissionError);
    };
    let profile = get_profile_by_id(db_client, *account_id).await?;
    if profile.is_local() {
        let account =
            get_managed_account_by_id(db_client, profile.id).await?;
        delete_account(
            &config,
            db_client,
            &account,
        ).await?;
    } else {
        let deletion_queue = delete_profile(db_client, profile.id).await?;
        deletion_queue.into_job(db_client).await?;
    };
    // NOTE: Mastodon returns AdminAccount
    let empty = serde_json::json!({});
    Ok(HttpResponse::NoContent().json(empty))
}

pub fn admin_account_api_v1_scope() -> Scope {
    web::scope("/v1/admin/accounts")
        .service(delete_account_view)
}

pub fn admin_account_api_v2_scope() -> Scope {
    web::scope("/v2/admin/accounts")
        .service(account_list_view)
}
