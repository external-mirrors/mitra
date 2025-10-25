use actix_multipart::form::MultipartForm;
use actix_web::{
    post,
    web,
    Either,
    HttpResponse,
    Scope,
};
use uuid::Uuid;

use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    oauth::queries::create_oauth_app,
    oauth::types::{OauthAppData as DbOauthAppData},
};
use mitra_validators::oauth::validate_redirect_uri;

use crate::http::JsonOrForm;
use crate::mastodon_api::{
    errors::MastodonError,
    oauth::utils::generate_oauth_token,
};
use super::types::{OauthApp, CreateAppData, CreateAppMultipartForm};

// https://docs.joinmastodon.org/methods/apps/#create
#[post("")]
async fn create_app_view(
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: Either<
        JsonOrForm<CreateAppData>,
        // Some clients use multipart/form-data
        MultipartForm<CreateAppMultipartForm>,
    >,
) -> Result<HttpResponse, MastodonError> {
    let request_data = match request_data {
        Either::Left(data) => data.into_inner(),
        Either::Right(form) => form.into_inner().into(),
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let db_app_data = DbOauthAppData {
        app_name: request_data.client_name,
        website: request_data.website,
        scopes: request_data.scopes,
        redirect_uri: request_data.redirect_uris,
        client_id: Uuid::new_v4(),
        client_secret: generate_oauth_token(),
    };
    validate_redirect_uri(&db_app_data.redirect_uri)?;
    let db_app = create_oauth_app(db_client, db_app_data).await?;
    let app = OauthApp::from_db(db_app);
    Ok(HttpResponse::Ok().json(app))
}

pub fn application_api_scope() -> Scope {
    web::scope("/v1/apps")
        .service(create_app_view)
}
