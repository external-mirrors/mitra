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
use mitra_validators::oauth::{
    clean_scopes,
    validate_redirect_uri,
};

use crate::http::JsonOrForm;
use crate::mastodon_api::{
    errors::MastodonError,
    oauth::utils::generate_oauth_token,
};
use super::types::{OauthApp, CreateAppForm, CreateAppMultipartForm};

// https://docs.joinmastodon.org/methods/apps/#create
#[post("")]
async fn create_app_view(
    db_pool: web::Data<DatabaseConnectionPool>,
    app_form: Either<
        JsonOrForm<CreateAppForm>,
        // Some clients use multipart/form-data
        MultipartForm<CreateAppMultipartForm>,
    >,
) -> Result<HttpResponse, MastodonError> {
    let app_form = match app_form {
        Either::Left(form) => form.into_inner(),
        Either::Right(form) => form.into_inner().into(),
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let db_app_data = DbOauthAppData {
        app_name: app_form.client_name,
        website: app_form.website,
        scopes: clean_scopes(&app_form.scopes),
        redirect_uri: app_form.redirect_uris,
        client_id: Uuid::new_v4(),
        client_secret: generate_oauth_token(),
    };
    validate_redirect_uri(&db_app_data.redirect_uri)?;
    let db_app = create_oauth_app(db_client, db_app_data).await?;
    log::info!("registered app with scopes: {:?}", db_app.scopes);
    let app = OauthApp::from_db(db_app);
    Ok(HttpResponse::Ok().json(app))
}

pub fn application_api_scope() -> Scope {
    web::scope("/v1/apps")
        .service(create_app_view)
}
