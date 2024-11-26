use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::{
    database::errors::{DatabaseError, DatabaseTypeError},
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

impl Reaction {
    pub fn new(
        db_reaction: DbReaction,
        db_author: DbActorProfile,
        maybe_db_emoji: Option<DbEmoji>,
    ) -> Result<Self, DatabaseTypeError> {
        // Consistency checks
        db_author.check_consistency()?;
        if db_reaction.author_id != db_author.id {
            return Err(DatabaseTypeError);
        };
        if db_reaction.emoji_id != maybe_db_emoji.as_ref().map(|db_emoji| db_emoji.id) {
            return Err(DatabaseTypeError);
        };
        if db_reaction.emoji_id.is_some() && db_reaction.content.is_none() {
            return Err(DatabaseTypeError);
        };
        if let Some(ref db_emoji) = maybe_db_emoji {
            if db_author.is_local()
                && db_emoji.object_id.is_none()
                && !db_emoji.image.is_file()
            {
                // Related media must be stored locally
                return Err(DatabaseTypeError);
            };
        };
        let reaction = Self {
            id: db_reaction.id,
            author: db_author,
            content: db_reaction.content,
            emoji: maybe_db_emoji,
        };
        Ok(reaction)
    }
}

impl TryFrom<&Row> for Reaction {
    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_reaction = row.try_get("post_reaction")?;
        let db_author = row.try_get("author")?;
        let db_emoji = row.try_get("emoji")?;
        let reaction = Self::new(db_reaction, db_author, db_emoji)?;
        Ok(reaction)
    }
}
