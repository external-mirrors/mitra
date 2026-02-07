use actix_web::{
    delete,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::{
    adapters::users::delete_user,
};
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    profiles::queries::{delete_profile, get_profile_by_id},
    users::{
        queries::get_user_by_id,
        types::Permission,
    },
};

use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
};

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
    if profile.has_user_account() {
        let user = get_user_by_id(db_client, profile.id).await?;
        delete_user(
            &config,
            db_client,
            &user,
        ).await?;
    } else {
        let deletion_queue = delete_profile(db_client, profile.id).await?;
        deletion_queue.into_job(db_client).await?;
    };
    // NOTE: Mastodon returns AdminAccount
    let empty = serde_json::json!({});
    Ok(HttpResponse::NoContent().json(empty))
}

pub fn admin_account_api_scope() -> Scope {
    web::scope("/v1/admin/accounts")
        .service(delete_account_view)
}
