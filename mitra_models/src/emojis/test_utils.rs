use crate::media::types::{MediaInfo, PartialMediaInfo};

use super::types::CustomEmoji;

impl CustomEmoji {
    pub fn local_for_test(name: &str) -> Self {
        Self {
            id: Default::default(),
            emoji_name: name.to_owned(),
            hostname: None,
            image: PartialMediaInfo::from(MediaInfo::png_for_test()),
            object_id: None,
            updated_at: Default::default(),
        }
    }

    pub fn remote_for_test(name: &str, hostname: &str) -> Self {
        let object_id = format!("https://{hostname}/emoji");
        let media_info = {
            let mut media_info = MediaInfo::png_for_test();
            let MediaInfo::File { ref mut url, .. } = media_info else {
                unreachable!();
            };
            *url = Some(object_id.clone());
            media_info
        };
        Self {
            id: Default::default(),
            emoji_name: name.to_owned(),
            hostname: Some(hostname.to_owned()),
            image: PartialMediaInfo::from(media_info),
            object_id: Some(object_id),
            updated_at: Default::default(),
        }
    }
}
