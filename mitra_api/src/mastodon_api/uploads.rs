use apx_core::base64;

use mitra_services::media::{
    MediaStorage,
    MediaStorageError,
};
use mitra_utils::files::{FileInfo, FileSize};

use super::errors::MastodonError;

#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error(transparent)]
    WriteError(#[from] MediaStorageError),

    #[error("base64 decoding error")]
    Base64DecodingError(#[from] base64::DecodeError),

    #[error("file size must be less than {limit}")]
    TooLarge { limit: FileSize },

    #[error("no media type")]
    NoMediaType,

    #[error("invalid media type {0}")]
    InvalidMediaType(String),
}

impl From<UploadError> for MastodonError {
    fn from(error: UploadError) -> Self {
        match error {
            UploadError::WriteError(error) => {
                MastodonError::from_internal(error)
            },
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
) -> Result<FileInfo, UploadError> {
    let file_data = base64::decode(b64data)?;
    if file_data.len() > file_size_limit {
        return Err(UploadError::TooLarge {
            limit: FileSize::new(file_size_limit),
        });
    };
    let media_type = media_type.to_string();
    if !allowed_media_types.contains(&media_type.as_str()) {
        return Err(UploadError::InvalidMediaType(media_type));
    };
    let file_info = storage.save_file(file_data, &media_type)?;
    Ok(file_info)
}
