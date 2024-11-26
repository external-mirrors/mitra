use serde::Serialize;

use mitra_models::emojis::types::DbEmoji;

use crate::mastodon_api::media_server::ClientMediaServer;

/// https://docs.joinmastodon.org/entities/CustomEmoji/
#[derive(Serialize)]
pub struct CustomEmoji {
    pub shortcode: String,
    pub url: String,
    static_url: String,
    visible_in_picker: bool,
}

impl CustomEmoji {
    pub fn from_db(media_server: &ClientMediaServer, emoji: DbEmoji) -> Self {
        let image_url = media_server.url_for(&emoji.image);
        Self {
            shortcode: emoji.emoji_name,
            url: image_url.clone(),
            static_url: image_url,
            visible_in_picker: true,
        }
    }
}
