use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::oauth::types::OauthToken;

#[derive(Deserialize)]
pub struct PasswordChangeRequest {
    pub new_password: String,
}

#[derive(Serialize)]
pub struct Session {
    pub id: i32,
    client_name: Option<String>,
    created_at: DateTime<Utc>,
    pub is_current: bool,
}

impl Session {
    pub fn from_db(token: OauthToken) -> Self {
        Self {
            id: token.id,
            client_name: token.client_name,
            created_at: token.created_at,
            is_current: false,
        }
    }
}

#[derive(Deserialize)]
pub struct AddAliasRequest {
    pub acct: String,
}

#[derive(Deserialize)]
pub struct RemoveAliasRequest {
    pub actor_id: String,
}

#[derive(Deserialize)]
pub struct ImportFollowsRequest {
    pub follows_csv: String,
}

#[derive(Deserialize)]
pub struct ImportFollowersRequest {
    pub from_actor_id: String,
    pub followers_csv: String,
}

#[derive(Deserialize)]
pub struct MoveFollowersRequest {
    pub target_acct: String,
}
