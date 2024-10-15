use serde::{Deserialize, Serialize};

use mitra_utils::files::FileInfo;

// Same field names in ProfileImage and EmojiImage
#[derive(Clone)]
pub struct MediaInfo {
    pub file_name: String,
    pub file_size: usize,
    pub media_type: String,
}

impl From<FileInfo> for MediaInfo {
    fn from(file_info: FileInfo) -> Self {
        Self {
            file_name: file_info.name,
            file_size: file_info.size,
            media_type: file_info.media_type,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct DeletionQueue {
    pub files: Vec<String>,
    pub ipfs_objects: Vec<String>,
}
