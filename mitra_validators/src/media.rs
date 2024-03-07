use super::errors::ValidationError;

const MEDIA_DESCRIPTION_LENGTH_MAX: usize = 2000;

pub fn validate_media_description(description: &str) -> Result<(), ValidationError> {
    if description.len() > MEDIA_DESCRIPTION_LENGTH_MAX {
        return Err(ValidationError("media description is too long"));
    };
    Ok(())
}
