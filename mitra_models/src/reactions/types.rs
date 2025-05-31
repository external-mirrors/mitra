use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::{
    database::errors::DatabaseError,
    emojis::types::DbEmoji,
    posts::types::Visibility,
    profiles::types::DbActorProfile,
};

#[derive(FromSql)]
#[postgres(name = "post_reaction")]
pub struct DbReaction {
    pub id: Uuid,
    pub author_id: Uuid,
    pub post_id: Uuid,
    pub content: Option<String>,
    pub emoji_id: Option<Uuid>,
    pub visibility: Visibility,
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
    pub visibility: Visibility,
    pub activity_id: Option<String>,
}

pub struct Reaction {
    pub id: Uuid,
    pub author: DbActorProfile,
    pub content: Option<String>,
    pub emoji: Option<DbEmoji>,
}

impl TryFrom<&Row> for Reaction {
    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let id = row.try_get("id")?;
        let author = row.try_get("author")?;
        let content = row.try_get("content")?;
        let emoji = row.try_get("emoji")?;
        let reaction = Self { id, author, content, emoji };
        Ok(reaction)
    }
}
