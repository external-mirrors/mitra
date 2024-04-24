use mitra_utils::http_url::HttpUrl;

use super::errors::ValidationError;

const OBJECT_ID_SIZE_MAX: usize = 2000;

// Object ID is an IRI
// https://www.w3.org/TR/activitystreams-core/#urls
// TODO: allow only canonical URLs
pub fn validate_object_id(object_id: &str) -> Result<(), ValidationError> {
    if object_id.len() > OBJECT_ID_SIZE_MAX {
        return Err(ValidationError("object ID is too long"));
    };
    // Validates HTTP(S) URL (ap:// URLs are not allowed)
    HttpUrl::parse(object_id)
        .map_err(|_| ValidationError("invalid object ID"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_object_id() {
        let object_id = "https://social.example/users/alice";
        let result = validate_object_id(object_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_object_id_i2p() {
        let object_id = "http://social.i2p/users/alice";
        let result = validate_object_id(object_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_object_id_ftp() {
        let object_id = "ftp://ftp.social.example/";
        let result = validate_object_id(object_id);
        assert!(result.is_err());
    }
}
