use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

use crate::media::types::PartialMediaInfo;

pub enum AttachmentType {
    Unknown,
    Image,
    Video,
    Audio,
}

#[derive(Clone, FromSql)]
#[postgres(name = "media_attachment")]
pub struct DbMediaAttachment {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub media: PartialMediaInfo,
    pub description: Option<String>,
    pub ipfs_cid: Option<String>,
    pub post_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl DbMediaAttachment {
    pub fn attachment_type(&self) -> AttachmentType {
        match self.media.media_type() {
            Some(media_type) => {
                if media_type.starts_with("image/") {
                    AttachmentType::Image
                } else if media_type.starts_with("video/") {
                    AttachmentType::Video
                } else if media_type.starts_with("audio/") {
                    AttachmentType::Audio
                } else {
                    AttachmentType::Unknown
                }
            },
            None => AttachmentType::Unknown,
        }
    }
}
