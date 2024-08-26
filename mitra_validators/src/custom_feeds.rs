use super::errors::ValidationError;

const CUSTOM_FEED_NAME_SIZE_MAX: usize = 200; // database column limit

pub fn validate_custom_feed_name(name: &str) -> Result<(), ValidationError> {
    if name.trim().is_empty() {
        return Err(ValidationError("feed name is empty"));
    };
    if name.len() > CUSTOM_FEED_NAME_SIZE_MAX {
        return Err(ValidationError("feed name is too long"));
    };
    Ok(())
}
