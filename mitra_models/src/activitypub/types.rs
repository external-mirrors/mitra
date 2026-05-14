use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "activitypub_object")]
pub struct ActivityPubObject {
    pub object_id: String,
    pub object_data: JsonValue,
    profile_id: Option<Uuid>,
    post_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}
