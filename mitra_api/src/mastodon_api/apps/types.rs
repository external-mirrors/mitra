use actix_multipart::form::{text::Text, MultipartForm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_models::oauth::types::{OauthApp as DbOauthApp};

#[derive(Deserialize)]
pub struct CreateAppData {
    pub client_name: String,
    pub redirect_uris: String,
    pub scopes: String,
    pub website: Option<String>,
}

#[derive(MultipartForm)]
pub struct CreateAppMultipartForm {
    client_name: Text<String>,
    redirect_uris: Text<String>,
    scopes: Text<String>,
    website: Option<Text<String>>,
}

impl From<CreateAppMultipartForm> for CreateAppData {
    fn from(form: CreateAppMultipartForm) -> Self {
        Self {
            client_name: form.client_name.into_inner(),
            redirect_uris: form.redirect_uris.into_inner(),
            scopes: form.scopes.into_inner(),
            website: form.website.map(|value| value.into_inner()),
        }
    }
}

// https://docs.joinmastodon.org/entities/Application/
#[derive(Serialize)]
pub struct OauthApp {
    pub id: String,
    pub name: String,
    pub website: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    pub redirect_uris: Vec<String>,
    pub client_id: Option<Uuid>,
    pub client_secret: Option<String>,
}

impl OauthApp {
    pub fn from_db(db_app: DbOauthApp) -> Self {
        Self {
            id: db_app.id.to_string(),
            name: db_app.app_name,
            website: db_app.website,
            scopes: db_app.scopes,
            redirect_uri: db_app.redirect_uri.clone(),
            redirect_uris: vec![db_app.redirect_uri],
            client_id: Some(db_app.client_id),
            client_secret: Some(db_app.client_secret),
        }
    }
}
