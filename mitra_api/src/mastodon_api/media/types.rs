use actix_multipart::form::{
    bytes::Bytes,
    text::Text,
    MultipartForm,
};
use apx_core::{
    base64,
    media_type::sniff_media_type,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_models::attachments::types::{
    AttachmentType,
    DbMediaAttachment,
};

use crate::mastodon_api::media_server::ClientMediaServer;

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
        const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";
        let media_type = form.file.content_type
            .map(|mime| mime.essence_str().to_string())
            // Ignore if content type is application/octet-stream
            .filter(|media_type| media_type != APPLICATION_OCTET_STREAM)
            // Workaround for clients that don't provide content type
            .or_else(|| sniff_media_type(&form.file.data))
            .unwrap_or(APPLICATION_OCTET_STREAM.to_string());
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
    pub fn from_db(
        media_server: &ClientMediaServer,
        db_attachment: DbMediaAttachment,
    ) -> Self {
        let attachment_type_mastodon = match db_attachment.attachment_type() {
            AttachmentType::Unknown => "unknown",
            AttachmentType::Image => "image",
            AttachmentType::Video => "video",
            AttachmentType::Audio => "audio",
        };
        let attachment_url = media_server.url_for(&db_attachment.media);
        Self {
            id: db_attachment.id,
            attachment_type: attachment_type_mastodon.to_string(),
            url: attachment_url.clone(),
            preview_url: attachment_url,
            description: db_attachment.description,
        }
    }
}
