use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::Deserialize;
use uuid::Uuid;

use crate::media::types::PartialMediaInfo;

#[derive(Clone, Deserialize, FromSql)]
#[postgres(name = "emoji")]
pub struct DbEmoji {
    pub id: Uuid,
    pub emoji_name: String,
    pub hostname: Option<String>,
    pub image: PartialMediaInfo,
    pub object_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl DbEmoji {
    pub fn shortcode(&self) -> String {
        format!(":{}:", self.emoji_name)
    }
}
