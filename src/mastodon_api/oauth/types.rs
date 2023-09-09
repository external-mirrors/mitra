use actix_multipart::form::{text::Text, MultipartForm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AuthorizationRequest {
    pub username: String,
    pub password: String,
}

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
    code: Text<String>,
}

impl From<TokenRequestMultipartForm> for TokenRequest {
    fn from(form: TokenRequestMultipartForm) -> Self {
        Self {
            grant_type: form.grant_type.into_inner(),
            code: Some(form.code.into_inner()),
            username: None,
            password: None,
            message: None,
            signature: None,
        }
    }
}

/// https://docs.joinmastodon.org/entities/token/
#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    pub created_at: i64,
}

impl TokenResponse {
    pub fn new(access_token: String, created_at: i64) -> Self {
        Self {
            access_token,
            token_type: "Bearer".to_string(),
            scope: "read write follow".to_string(),
            created_at,
        }
    }
}

#[derive(Deserialize)]
pub struct RevocationRequest {
    pub token: String,
}
