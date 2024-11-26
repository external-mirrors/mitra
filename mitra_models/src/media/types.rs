use serde::{Deserialize, Serialize};

use mitra_utils::files::FileInfo;

use crate::database::json_macro::{json_from_sql, json_to_sql};

#[derive(Clone)]
pub enum MediaInfo {
    File {
        file_info: FileInfo,
        url: Option<String>,
    },
    Link {
        media_type: String,
        url: String,
    },
}

impl MediaInfo {
    pub fn local(file_info: FileInfo) -> Self {
        Self::File { file_info, url: None }
    }

    pub fn remote(file_info: FileInfo, url: String) -> Self {
        Self::File { file_info, url: Some(url) }
    }

    pub fn link(media_type: String, url: String) -> Self {
        Self::Link { media_type, url }
    }
}

// Same field names in database
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PartialFileInfo {
    pub file_name: String,
    pub file_size: Option<usize>,
    pub digest: Option<[u8; 32]>,
    pub media_type: Option<String>,
}

impl From<FileInfo> for PartialFileInfo {
    fn from(file_info: FileInfo) -> Self {
        Self {
            file_name: file_info.file_name,
            file_size: Some(file_info.file_size),
            digest: Some(file_info.digest),
            media_type: Some(file_info.media_type),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum PartialMediaInfo {
    #[serde(rename = "file")]
    File {
        #[serde(flatten)]
        file_info: PartialFileInfo,
        url: Option<String>,
    },
    #[serde(rename = "link")]
    Link {
        media_type: String,
        url: String,
    },
}

impl PartialMediaInfo {
    pub fn file_info(&self) -> Option<&PartialFileInfo> {
        match self {
            Self::File { file_info, .. } => Some(file_info),
            Self::Link { .. } => None,
        }
    }

    pub fn expect_file_info(&self) -> &PartialFileInfo {
        self.file_info().expect("media should be stored locally")
    }

    pub(crate) fn into_file_name(self) -> Option<String> {
        match self {
            Self::File { file_info, .. } => Some(file_info.file_name),
            Self::Link { .. } => None,
        }
    }

    pub fn media_type(&self) -> Option<&String> {
        match self {
            Self::File { file_info, .. } => file_info.media_type.as_ref(),
            Self::Link { media_type, .. } => Some(media_type),
        }
    }

    pub fn url(&self) -> Option<&String> {
        match self {
            Self::File { url, .. } => url.as_ref(),
            Self::Link { url, .. } => Some(url),
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self, Self::File { .. })
    }
}

impl From<MediaInfo> for PartialMediaInfo {
    fn from(media_info: MediaInfo) -> Self {
        match media_info {
            MediaInfo::File { file_info, url } => {
                Self::File {
                    file_info: PartialFileInfo::from(file_info),
                    url: url,
                }
            },
            MediaInfo::Link { media_type, url } => {
                Self::Link { media_type, url }
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_info_file_serialization() {
        let file_info = FileInfo {
            file_name: "test.png".to_owned(),
            file_size: 10000,
            digest: [0; 32],
            media_type: "image/png".to_owned(),
        };
        let media = PartialMediaInfo::from(MediaInfo::local(file_info));
        let value = serde_json::to_value(media.clone()).unwrap();
        let expected_value = serde_json::json!({
            "type": "file",
            "file_name": "test.png",
            "file_size": 10000,
            "digest": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "media_type": "image/png",
            "url": null,
        });
        assert_eq!(value, expected_value);
        let media_deserialized: PartialMediaInfo =
            serde_json::from_value(value).unwrap();
        assert_eq!(media_deserialized, media);
    }

    #[test]
    fn test_media_info_link_serialization() {
        let media_type = "image/png".to_owned();
        let url = "https://social.example/image.png".to_owned();
        let media = PartialMediaInfo::from(MediaInfo::link(media_type, url));
        let value = serde_json::to_value(media.clone()).unwrap();
        let expected_value = serde_json::json!({
            "type": "link",
            "media_type": "image/png",
            "url": "https://social.example/image.png",
        });
        assert_eq!(value, expected_value);
        let media_deserialized: PartialMediaInfo =
            serde_json::from_value(value).unwrap();
        assert_eq!(media_deserialized, media);
    }
}
