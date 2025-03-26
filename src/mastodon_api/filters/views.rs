// https://docs.joinmastodon.org/methods/filters/
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

#[get("")]
async fn filters_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let data = serde_json::json!([]);
    Ok(HttpResponse::Ok().json(data))
}

pub fn filter_api_scope() -> Scope {
    web::scope("/v2/filters")
        .service(filters_view)
}
