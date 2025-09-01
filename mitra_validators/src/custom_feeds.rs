use mitra_utils::unicode::trim_invisible;

use super::errors::ValidationError;

const CUSTOM_FEED_NAME_SIZE_MAX: usize = 200; // database column limit

pub fn clean_custom_feed_name(name: &str) -> &str {
    // Sanitization is not needed because `name` is a plain-text field
    trim_invisible(name)
}

pub fn validate_custom_feed_name(name: &str) -> Result<(), ValidationError> {
    if trim_invisible(name).is_empty() {
        return Err(ValidationError("feed name is empty"));
    };
    if name.len() > CUSTOM_FEED_NAME_SIZE_MAX {
        return Err(ValidationError("feed name is too long"));
    };
    Ok(())
}
