use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

#[derive(FromSql)]
#[postgres(name = "post_reaction")]
pub struct DbReaction {
    pub id: Uuid,
    pub author_id: Uuid,
    pub post_id: Uuid,
    pub content: Option<String>,
    pub emoji_id: Option<Uuid>,
    pub activity_id: Option<String>,
    #[allow(dead_code)]
    has_deprecated_ap_id: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct ReactionData {
    pub author_id: Uuid,
    pub post_id: Uuid,
    pub content: Option<String>,
    pub emoji_id: Option<Uuid>,
    pub activity_id: Option<String>,
}
