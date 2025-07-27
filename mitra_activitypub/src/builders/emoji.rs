use chrono::{DateTime, Utc};
use serde::Serialize;

use mitra_models::emojis::types::DbEmoji;
use mitra_services::media::MediaServer;

use crate::{
    identifiers::local_emoji_id,
    vocabulary::{EMOJI, IMAGE},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmojiImage {
    #[serde(rename = "type")]
    object_type: String,
    url: String,
    media_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Emoji {
    #[serde(rename = "type")]
    object_type: String,
    id: String,
    name: String,
    icon: EmojiImage,
    updated: DateTime<Utc>,
}

pub fn build_emoji(
    instance_url: &str,
    media_server: &MediaServer,
    db_emoji: &DbEmoji,
) -> Emoji {
    Emoji {
        object_type: EMOJI.to_string(),
        id: local_emoji_id(instance_url, &db_emoji.emoji_name),
        name: db_emoji.shortcode(),
        icon: EmojiImage {
            object_type: IMAGE.to_string(),
            url: media_server.url_for(&db_emoji.image.file_name),
            media_type: db_emoji.image.media_type.clone(),
        },
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
        let media_server = MediaServer::for_test(instance_url);
        let db_emoji = DbEmoji::local_for_test("test");
        let emoji = build_emoji(instance_url, &media_server, &db_emoji);
        let emoji_value = serde_json::to_value(emoji).unwrap();
        let expected_value = json!({
            "type": "Emoji",
            "id": "https://social.example/objects/emojis/test",
            "name": ":test:",
            "icon": {
                "type": "Image",
                "url": "https://social.example/media/test.png",
                "mediaType": "image/png",
            },
            "updated": "1970-01-01T00:00:00Z",
        });
        assert_eq!(emoji_value, expected_value);
    }
}
