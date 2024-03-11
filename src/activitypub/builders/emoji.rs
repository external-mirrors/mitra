use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_activitypub::{
    identifiers::local_emoji_id,
};
use mitra_models::emojis::types::DbEmoji;
use mitra_services::media::get_file_url;

use crate::activitypub::vocabulary::{EMOJI, IMAGE};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiImage {
    #[serde(rename = "type")]
    object_type: String,
    pub url: String,
    pub media_type: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Emoji {
    #[serde(rename = "type")]
    object_type: String,
    pub icon: EmojiImage,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub updated: DateTime<Utc>,
}

pub fn build_emoji(instance_url: &str, db_emoji: &DbEmoji) -> Emoji {
    Emoji {
        object_type: EMOJI.to_string(),
        icon: EmojiImage {
            object_type: IMAGE.to_string(),
            url: get_file_url(instance_url, &db_emoji.image.file_name),
            media_type: Some(db_emoji.image.media_type.clone()),
        },
        id: local_emoji_id(instance_url, &db_emoji.emoji_name),
        name: format!(":{}:", db_emoji.emoji_name),
        updated: db_emoji.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_build_emoji() {
        let instance_url = "https://social.example";
        let updated_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let db_emoji = DbEmoji {
            emoji_name: "test".to_string(),
            updated_at,
            ..Default::default()
        };
        let emoji = build_emoji(instance_url, &db_emoji);
        let emoji_value = serde_json::to_value(emoji).unwrap();
        let expected_value = json!({
            "id": "https://social.example/objects/emojis/test",
            "type": "Emoji",
            "name": ":test:",
            "icon": {
                "type": "Image",
                "url": "https://social.example/media/",
                "mediaType": "",
            },
            "updated": "2023-02-24T23:36:38Z",
        });
        assert_eq!(emoji_value, expected_value);
    }
}
