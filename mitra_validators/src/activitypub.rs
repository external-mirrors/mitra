use mitra_utils::{
    ap_url::{is_ap_url, ApUrl},
    http_url::HttpUrl,
};

use super::errors::ValidationError;

const OBJECT_ID_SIZE_MAX: usize = 2000;

// TODO: FEP-EF61: import from mitra_federation?
const GATEWAY_PATH_PREFIX: &str = "/.well-known/apgateway/";

// Object ID is an URI
// https://www.w3.org/TR/activitypub/#obj-id
pub fn validate_object_id(object_id: &str) -> Result<(), ValidationError> {
    if object_id.len() > OBJECT_ID_SIZE_MAX {
        return Err(ValidationError("object ID is too long"));
    };
    if is_ap_url(object_id) {
        // Validate 'ap' URL
        ApUrl::parse(object_id)
            .map_err(|_| ValidationError("invalid object ID"))?;
        // TODO: FEP-EF61: allow 'ap' URLs
        return Err(ValidationError("object ID is 'ap' URL"));
    } else {
        // Validate HTTP(S) URL
        let http_url = HttpUrl::parse(object_id)
            .map_err(|_| ValidationError("invalid object ID"))?;
        if http_url.path().starts_with(GATEWAY_PATH_PREFIX) {
            return Err(ValidationError("object ID is not canonical"));
        };
    };
    Ok(())
}

pub fn validate_gateway_url(url: &str) -> Result<(), ValidationError> {
    let http_url = HttpUrl::parse(url)
        .map_err(|_| ValidationError("invalid gateway URL"))?;
    if http_url.origin() != url {
        return Err(ValidationError("invalid gateway URL"));
    };
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
    fn test_validate_object_id_ap() {
        let object_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let result = validate_object_id(object_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_object_id_ap_compatible() {
        let object_id = "http://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let result = validate_object_id(object_id);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, "object ID is not canonical");
    }

    #[test]
    fn test_validate_object_id_ftp() {
        let object_id = "ftp://ftp.social.example/";
        let result = validate_object_id(object_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_gateway_url() {
        let url = "https://social.example";
        let result = validate_gateway_url(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_gateway_url_trailing_slash() {
        let url = "https://social.example/";
        let result = validate_gateway_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_gateway_url_with_path() {
        let url = "https://social.example/.well-known/apgateway";
        let result = validate_gateway_url(url);
        assert!(result.is_err());
    }
}
