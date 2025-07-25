use serde::{Deserialize, Serialize};

use mitra_utils::files::FileInfo;

use crate::database::json_macro::{json_from_sql, json_to_sql};

#[derive(Clone)]
pub struct MediaInfo {
    pub file_name: String,
    pub file_size: usize,
    pub digest: [u8; 32],
    pub media_type: String,
    pub url: Option<String>,
}

impl MediaInfo {
    pub fn local(file_info: FileInfo) -> Self {
        Self {
            file_name: file_info.file_name,
            file_size: file_info.file_size,
            digest: file_info.digest,
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

// Same field names in database
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PartialMediaInfo {
    pub file_name: String,
    pub file_size: Option<usize>,
    pub digest: Option<[u8; 32]>,
    pub media_type: Option<String>,
    pub url: Option<String>,
}

impl From<MediaInfo> for PartialMediaInfo {
    fn from(media_info: MediaInfo) -> Self {
        Self {
            file_name: media_info.file_name,
            file_size: Some(media_info.file_size),
            digest: Some(media_info.digest),
            media_type: Some(media_info.media_type),
            url: media_info.url,
        }
    }
}

json_from_sql!(PartialMediaInfo);
json_to_sql!(PartialMediaInfo);

#[derive(Deserialize, Serialize)]
pub struct DeletionQueue {
    pub files: Vec<String>,
    pub ipfs_objects: Vec<String>,
}
