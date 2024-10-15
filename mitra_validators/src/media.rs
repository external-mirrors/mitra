use apx_core::http_url::HttpUrl;

use super::errors::ValidationError;

const MEDIA_URL_LENGTH_MAX: usize = 2000;
const MEDIA_DESCRIPTION_LENGTH_MAX: usize = 3000;

pub fn validate_media_url(url: &str) -> Result<(), ValidationError> {
    HttpUrl::parse(url)
        .map_err(|_| ValidationError("invalid media URL"))?;
    if url.len() > MEDIA_URL_LENGTH_MAX {
        return Err(ValidationError("media URL is too long"));
    };
    Ok(())
}

pub fn validate_media_description(description: &str) -> Result<(), ValidationError> {
    if description.len() > MEDIA_DESCRIPTION_LENGTH_MAX {
        return Err(ValidationError("media description is too long"));
    };
    Ok(())
}
