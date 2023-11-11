use mitra_utils::base64;

use crate::media::{
    MediaStorage,
    MediaStorageError,
};

use super::errors::MastodonError;

#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error(transparent)]
    WriteError(#[from] MediaStorageError),

    #[error("base64 decoding error")]
    Base64DecodingError(#[from] base64::DecodeError),

    #[error("file is too large")]
    TooLarge,

    #[error("no media type")]
    NoMediaType,

    #[error("invalid media type {0}")]
    InvalidMediaType(String),
}

impl From<UploadError> for MastodonError {
    fn from(error: UploadError) -> Self {
        match error {
            UploadError::WriteError(_) => MastodonError::InternalError,
            other_error => {
                MastodonError::ValidationError(other_error.to_string())
            },
        }
    }
}

pub fn save_b64_file(
    b64data: &str,
    media_type: &str,
    storage: &MediaStorage,
    file_size_limit: usize,
    allowed_media_types: &[&str],
) -> Result<(String, usize, String), UploadError> {
    let file_data = base64::decode(b64data)?;
    let file_size = file_data.len();
    if file_size > file_size_limit {
        return Err(UploadError::TooLarge);
    };
    let media_type = media_type.to_string();
    if !allowed_media_types.contains(&media_type.as_str()) {
        return Err(UploadError::InvalidMediaType(media_type));
    };
    let file_name = storage.save_file(file_data, &media_type)?;
    Ok((file_name, file_size, media_type))
}
