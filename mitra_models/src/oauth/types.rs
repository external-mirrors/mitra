use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::{Error as PgError, Row};
use uuid::Uuid;

#[derive(FromSql)]
#[postgres(name = "oauth_application")]
pub struct OauthApp {
    pub id: i32,
    pub app_name: String,
    pub website: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    pub client_id: Uuid,
    pub client_secret: String,
    pub created_at: DateTime<Utc>,
}

#[cfg_attr(test, derive(Default))]
pub struct OauthAppData {
    pub app_name: String,
    pub website: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    pub client_id: Uuid,
    pub client_secret: String,
}

pub struct OauthToken {
    pub id: i32,
    pub client_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl TryFrom<Row> for OauthToken {
    type Error = PgError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let token_info = Self {
            id: row.try_get("id")?,
            client_name: row.try_get("app_name")?,
            created_at: row.try_get("created_at")?,
            expires_at: row.try_get("expires_at")?,
        };
        Ok(token_info)
    }
}
