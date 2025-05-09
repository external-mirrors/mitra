use apx_core::{
    http_url::HttpUrl,
    url::canonical::{parse_url, Url},
};

use super::errors::ValidationError;

const OBJECT_ID_SIZE_MAX: usize = 2000;

// Object ID is an URI
// https://www.w3.org/TR/activitypub/#obj-id
fn _validate_any_object_id(
    object_id: &str,
    allow_ap: bool,
) -> Result<(), ValidationError> {
    if object_id.len() > OBJECT_ID_SIZE_MAX {
        return Err(ValidationError("object ID is too long"));
    };
    let (canonical_object_id, maybe_gateway) = parse_url(object_id)
        .map_err(|_| ValidationError("invalid object ID"))?;
    if maybe_gateway.is_some() {
        return Err(ValidationError("object ID is not canonical"));
    };
    if !allow_ap && matches!(canonical_object_id, Url::Ap(_)) {
        return Err(ValidationError("object ID is 'ap' URL"));
    };
    Ok(())
}

pub fn validate_object_id(object_id: &str) -> Result<(), ValidationError> {
    // Doesn't allow 'ap' URLs
    _validate_any_object_id(object_id, false)
}

pub fn validate_any_object_id(object_id: &str) -> Result<(), ValidationError> {
    // Allows 'ap' URLs
    _validate_any_object_id(object_id, true)
}

pub(crate) fn validate_origin(
    id_1: &str,
    id_2: &str,
) -> Result<(), ValidationError> {
    let origin_1 = Url::parse_canonical(id_1)
        .map_err(|_| ValidationError("invalid object ID"))?
        .origin();
    let origin_2 = Url::parse_canonical(id_2)
        .map_err(|_| ValidationError("invalid object ID"))?
        .origin();
    if origin_1 != origin_2 {
        return Err(ValidationError("related ID has different origin"));
    };
    Ok(())
}

pub(crate) fn validate_endpoint_url(url: &str) -> Result<(), ValidationError> {
    HttpUrl::parse(url)
        .map_err(|_| ValidationError("invalid endpoint URL"))?;
    Ok(())
}

pub(crate) fn validate_gateway_url(url: &str) -> Result<(), ValidationError> {
    let http_url = HttpUrl::parse(url)
        .map_err(|_| ValidationError("invalid gateway URL"))?;
    if http_url.base() != url {
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
    fn test_validate_origin_http() {
        let object_id_1 = "https://server1.example/actor";
        let object_id_2 = "https://server1.example/actor/followers";
        let result = validate_origin(object_id_1, object_id_2);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_origin_ap() {
        let object_id_1 = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let object_id_2 = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/followers";
        let result = validate_origin(object_id_1, object_id_2);
        assert!(result.is_ok());
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
