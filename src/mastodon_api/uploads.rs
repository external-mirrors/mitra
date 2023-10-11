use std::path::Path;

use mitra_utils::base64;

use crate::media::{save_file, SUPPORTED_MEDIA_TYPES};

use super::errors::MastodonError;

#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error(transparent)]
    WriteError(#[from] std::io::Error),

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
    output_dir: &Path,
    file_size_limit: usize,
    maybe_expected_prefix: Option<&str>,
) -> Result<(String, usize, String), UploadError> {
    let file_data = base64::decode(b64data)?;
    let file_size = file_data.len();
    if file_size > file_size_limit {
        return Err(UploadError::TooLarge);
    };
    let media_type = media_type.to_string();
    if !SUPPORTED_MEDIA_TYPES.contains(&media_type.as_str()) {
        return Err(UploadError::InvalidMediaType(media_type));
    };
    if let Some(expected_prefix) = maybe_expected_prefix {
        if !media_type.starts_with(expected_prefix) {
            return Err(UploadError::InvalidMediaType(media_type));
        };
    };
    let file_name = save_file(
        file_data,
        output_dir,
        Some(&media_type),
    )?;
    Ok((file_name, file_size, media_type))
}
