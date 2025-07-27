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
pub struct EmojiImage {
    pub file_name: String,
    #[serde(default = "default_emoji_file_size")]
    pub file_size: usize,
    digest: Option<[u8; 32]>,
    pub media_type: String,
    pub url: Option<String>,
}

impl From<MediaInfo> for EmojiImage {
    fn from(media_info: MediaInfo) -> Self {
        Self {
            file_name: media_info.file_name,
            file_size: media_info.file_size,
            digest: Some(media_info.digest),
            media_type: media_info.media_type,
            url: media_info.url,
        }
    }
}

json_from_sql!(EmojiImage);
json_to_sql!(EmojiImage);

#[derive(Clone, Deserialize, FromSql)]
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
