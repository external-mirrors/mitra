use crate::media::types::MediaInfo;

use super::types::{DbEmoji, EmojiImage};

impl DbEmoji {
    pub fn local_for_test(name: &str) -> Self {
        DbEmoji {
            id: Default::default(),
            emoji_name: name.to_owned(),
            hostname: None,
            image: EmojiImage::from(MediaInfo::png_for_test()),
            object_id: None,
            updated_at: Default::default(),
        }
    }
}
