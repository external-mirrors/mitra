use crate::media::types::MediaInfo;

use super::types::{DbEmoji, EmojiImage};

impl DbEmoji {
    pub fn local_for_test(name: &str) -> Self {
        Self {
            id: Default::default(),
            emoji_name: name.to_owned(),
            hostname: None,
            image: EmojiImage::from(MediaInfo::png_for_test()),
            object_id: None,
            updated_at: Default::default(),
        }
    }

    pub fn remote_for_test(name: &str, hostname: &str) -> Self {
        let url = format!("https://{hostname}/emoji");
        let media_info = MediaInfo {
            url: Some(url.clone()),
            ..MediaInfo::png_for_test()
        };
        Self {
            id: Default::default(),
            emoji_name: name.to_owned(),
            hostname: Some(hostname.to_owned()),
            image: EmojiImage::from(media_info),
            object_id: Some(url),
            updated_at: Default::default(),
        }
    }
}
