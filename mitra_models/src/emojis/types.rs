use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    database::json_macro::{json_from_sql, json_to_sql},
    media::types::MediaInfo,
};

// Migration
fn default_emoji_file_size() -> usize { 250 * 1000 }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(any(test, feature = "test-utils"), derive(Default))]
pub struct EmojiImage {
    pub file_name: String,
    #[serde(default = "default_emoji_file_size")]
    pub file_size: usize,
    pub media_type: String,
}

impl From<MediaInfo> for EmojiImage {
    fn from(media_info: MediaInfo) -> Self {
        Self {
            file_name: media_info.file_name,
            file_size: media_info.file_size,
            media_type: media_info.media_type,
        }
    }
}

json_from_sql!(EmojiImage);
json_to_sql!(EmojiImage);

#[derive(Clone, Deserialize, FromSql)]
#[cfg_attr(feature = "test-utils", derive(Default))]
#[postgres(name = "emoji")]
pub struct DbEmoji {
    pub id: Uuid,
    pub emoji_name: String,
    pub hostname: Option<String>,
    pub image: EmojiImage,
    pub object_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl DbEmoji {
    pub fn shortcode(&self) -> String {
        format!(":{}:", self.emoji_name)
    }
}
