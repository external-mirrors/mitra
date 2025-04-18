use actix_multipart::form::{text::Text, MultipartForm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

/// https://docs.joinmastodon.org/entities/Application/
#[derive(Serialize)]
pub struct OauthApp {
    pub id: String,
    pub name: String,
    pub website: Option<String>,
    pub redirect_uri: String,
    pub client_id: Option<Uuid>,
    pub client_secret: Option<String>,
}
