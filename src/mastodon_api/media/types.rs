use actix_multipart::form::{
    bytes::Bytes,
    text::Text,
    MultipartForm,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use apx_core::{
    base64,
    media_type::sniff_media_type,
};
use mitra_models::attachments::types::{
    AttachmentType,
    DbMediaAttachment,
};
use mitra_services::media::get_file_url;

#[derive(Deserialize)]
pub struct AttachmentData {
    // base64-encoded file (not compatible with Mastodon)
    pub file: String,
    pub media_type: String,
    pub description: Option<String>,
}

#[derive(MultipartForm)]
pub struct AttachmentDataMultipartForm {
    file: Bytes,
    description: Option<Text<String>>,
}

impl From<AttachmentDataMultipartForm> for AttachmentData {
    fn from(form: AttachmentDataMultipartForm) -> Self {
        let media_type = form.file.content_type
            .and_then(|mime| {
                let media_type = mime.essence_str().to_string();
                if media_type == "application/octet-stream" {
                    // Workaround for Bloat-FE
                    sniff_media_type(&form.file.data)
                } else {
                    Some(media_type)
                }
            })
            // Use application/octet-stream as fallback type
            .unwrap_or("application/octet-stream".to_string());
        Self {
            file: base64::encode(form.file.data),
            media_type: media_type,
            description: form.description.map(|text| text.into_inner()),
        }
    }
}

#[derive(Deserialize)]
pub struct AttachmentUpdateData {
    pub description: Option<String>,
}

/// https://docs.joinmastodon.org/entities/attachment/
#[derive(Serialize)]
pub struct Attachment {
    pub id: Uuid,

    #[serde(rename = "type")]
    pub attachment_type: String,

    pub url: String,
    pub preview_url: String,
    description: Option<String>,
}

impl Attachment {
    pub fn from_db(base_url: &str, db_attachment: DbMediaAttachment) -> Self {
        let attachment_type =
            AttachmentType::from_media_type(db_attachment.media_type);
        let attachment_type_mastodon = match attachment_type {
            AttachmentType::Unknown => "unknown",
            AttachmentType::Image => "image",
            AttachmentType::Video => "video",
            AttachmentType::Audio => "audio",
        };
        let attachment_url = get_file_url(
            base_url,
            &db_attachment.file_name,
        );
        Self {
            id: db_attachment.id,
            attachment_type: attachment_type_mastodon.to_string(),
            url: attachment_url.clone(),
            preview_url: attachment_url,
            description: db_attachment.description,
        }
    }
}
