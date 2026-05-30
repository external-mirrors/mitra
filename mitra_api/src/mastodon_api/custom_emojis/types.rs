use serde::Serialize;

use mitra_models::emojis::types::{CustomEmoji as DbCustomEmoji};

use crate::mastodon_api::media_server::ClientMediaServer;

// https://docs.joinmastodon.org/entities/CustomEmoji/
#[derive(Serialize)]
pub struct CustomEmoji {
    pub shortcode: String,
    pub url: String,
    static_url: String,
    visible_in_picker: bool,
    category: Option<String>,
}

impl CustomEmoji {
    pub fn from_db(
        media_server: &ClientMediaServer,
        db_emoji: DbCustomEmoji,
    ) -> Self {
        let image_url = media_server.url_for(&db_emoji.image);
        Self {
            shortcode: db_emoji.emoji_name,
            url: image_url.clone(),
            static_url: image_url,
            visible_in_picker: true,
            category: db_emoji.category,
        }
    }
}
