use serde::{Deserialize, Serialize};

use mitra_utils::files::FileInfo;

// Same field names in ProfileImage and EmojiImage
#[derive(Clone)]
pub struct MediaInfo {
    pub file_name: String,
    pub file_size: usize,
    pub media_type: String,
    pub url: Option<String>,
}

impl MediaInfo {
    pub fn local(file_info: FileInfo) -> Self {
        Self {
            file_name: file_info.name,
            file_size: file_info.size,
            media_type: file_info.media_type,
            url: None,
        }
    }

    pub fn remote(file_info: FileInfo, url: String) -> Self {
        Self {
            url: Some(url),
            ..Self::local(file_info)
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct DeletionQueue {
    pub files: Vec<String>,
    pub ipfs_objects: Vec<String>,
}
