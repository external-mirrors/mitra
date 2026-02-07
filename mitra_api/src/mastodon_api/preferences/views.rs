// https://docs.joinmastodon.org/methods/preferences/
use actix_web::{
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
};

use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
};
use super::types::Preferences;

#[get("")]
async fn preferences_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let preferences = Preferences::new(current_user.shared_client_config);
    Ok(HttpResponse::Ok().json(preferences))
}

pub fn preferences_api_scope() -> Scope {
    web::scope("/v1/preferences")
        .service(preferences_view)
}
