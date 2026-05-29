use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::Deserialize;
use uuid::Uuid;

use crate::media::types::PartialMediaInfo;

#[derive(Clone, Deserialize, FromSql)]
#[postgres(name = "emoji")]
pub struct CustomEmoji {
    pub id: Uuid,
    pub emoji_name: String,
    #[allow(dead_code)]
    pub(super) hostname: Option<String>,
    pub image: PartialMediaInfo,
    pub category: Option<String>,
    pub object_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl CustomEmoji {
    pub fn shortcode(&self) -> String {
        format!(":{}:", self.emoji_name)
    }
}
