use actix_multipart::form::{text::Text, MultipartForm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AuthorizationRequest {
    pub username: String,
    pub password: String,
}

// https://datatracker.ietf.org/doc/html/rfc6749#section-4.1.1
#[derive(Deserialize)]
pub struct AuthorizationQueryParams {
    pub response_type: String,
    pub client_id: Uuid,
    pub redirect_uri: String,
    pub scope: String,
    pub state: Option<String>,
}

#[derive(Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,

    // Required if grant type is "authorization_code"
    pub code: Option<String>,
    #[allow(dead_code)]
    pub redirect_uri: Option<String>,
    pub client_id: Option<Uuid>,
    pub client_secret: Option<String>,

    // Required only with "password" grant type
    pub username: Option<String>,
    pub password: Option<String>,

    // EIP-4361 / CAIP-122 message and signature
    pub message: Option<String>,
    pub signature: Option<String>,
}

#[derive(MultipartForm)]
pub struct TokenRequestMultipartForm {
    grant_type: Text<String>,

    // Required if grant type is "authorization_code"
    code: Option<Text<String>>,
    redirect_uri: Option<Text<String>>,
    client_id: Option<Text<Uuid>>,
    client_secret: Option<Text<String>>,

    // Required only with "password" grant type
    username: Option<Text<String>>,
    password: Option<Text<String>>,
}

impl From<TokenRequestMultipartForm> for TokenRequest {
    fn from(form: TokenRequestMultipartForm) -> Self {
        Self {
            grant_type: form.grant_type.into_inner(),
            code: form.code.map(|value| value.into_inner()),
            redirect_uri: form.redirect_uri.map(|value| value.into_inner()),
            client_id: form.client_id.map(|value| value.into_inner()),
            client_secret: form.client_secret.map(|value| value.into_inner()),
            username: form.username.map(|value| value.into_inner()),
            password: form.password.map(|value| value.into_inner()),
            message: None,
            signature: None,
        }
    }
}

// https://datatracker.ietf.org/doc/html/rfc6749#section-5.1
// https://docs.joinmastodon.org/entities/token/
#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    pub created_at: i64,
    pub expires_in: u32,
}

impl TokenResponse {
    pub fn new(
        access_token: String,
        created_at: i64,
        expires_in: u32,
    ) -> Self {
        Self {
            access_token,
            token_type: "Bearer".to_string(),
            scope: "read write follow".to_string(),
            created_at,
            expires_in,
        }
    }
}

#[derive(Deserialize)]
pub struct RevocationRequest {
    pub token: String,
}
