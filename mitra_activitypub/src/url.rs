use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use mitra_utils::{
    ap_url::{is_ap_url, ApUrl},
    http_url::HttpUrl,
};
use mitra_validators::{
    errors::ValidationError,
};

pub(super) const GATEWAY_PATH_PREFIX: &str = "/.well-known/apgateway/";

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ObjectIdError(pub &'static str);

// TODO: FEP-EF61: rename to ID
pub enum Url {
    Http(HttpUrl),
    Ap(ApUrl),
}

fn with_gateway(ap_url: &ApUrl, gateway_url: &str) -> String {
    format!("{}{}{}", gateway_url, GATEWAY_PATH_PREFIX, ap_url.to_did_url())
}

impl Url {
    fn parse(value: &str) -> Result<Self, ObjectIdError> {
        let (url, _) = parse_url(value)?;
        Ok(url)
    }

    pub fn to_http_url(&self, maybe_gateway: Option<&str>) -> Option<String> {
        let url = match self {
            Self::Http(http_url) => http_url.to_string(),
            Self::Ap(ap_url) => {
                if let Some(gateway) = maybe_gateway {
                    with_gateway(ap_url, gateway)
                } else {
                    // Not enough context
                    return None;
                }
            },
        };
        Some(url)
    }
}

impl fmt::Display for Url {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(http_url) => write!(formatter, "{}", http_url),
            Self::Ap(ap_url) => write!(formatter, "{}", ap_url),
        }
    }
}

fn get_canonical_ap_url(
    http_url: HttpUrl,
) -> Result<(ApUrl, String), ObjectIdError> {
    let relative_http_url = http_url.to_relative();
    let did_url = relative_http_url
        .strip_prefix(GATEWAY_PATH_PREFIX)
        .ok_or(ObjectIdError("invalid gateway URL"))?;
    let ap_url = ApUrl::from_did_url(did_url)
        .map_err(ObjectIdError)?;
    let gateway = http_url.origin();
    Ok((ap_url, gateway))
}

pub fn parse_url(
    value: &str,
) -> Result<(Url, Option<String>), ObjectIdError> {
    let mut maybe_gateway = None;
    let url = if is_ap_url(value) {
        let ap_url = ApUrl::parse(value).map_err(ObjectIdError)?;
        Url::Ap(ap_url)
    } else {
        let http_url = HttpUrl::parse(value).map_err(ObjectIdError)?;
        // TODO: FEP-EF61: see also mitra_validators::activitypub
        if http_url.path().starts_with(GATEWAY_PATH_PREFIX) {
            let (ap_url, gateway) = get_canonical_ap_url(http_url)?;
            maybe_gateway = Some(gateway);
            Url::Ap(ap_url)
        } else {
            Url::Http(http_url)
        }
    };
    Ok((url, maybe_gateway))
}

impl FromStr for Url {
    type Err = ObjectIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

pub fn canonicalize_id(url: &str) -> Result<String, ValidationError> {
    let url = Url::parse(url).map_err(|error| ValidationError(error.0))?;
    Ok(url.to_string())
}

pub fn is_same_authority(id_1: &str, id_2: &str) -> Result<bool, ObjectIdError> {
    let id_1 = Url::parse(id_1)?;
    let id_2 = Url::parse(id_2)?;
    let is_same = match (id_1, id_2) {
        (Url::Http(http_url_1), Url::Http(http_url_2)) => {
            http_url_1.authority() == http_url_2.authority()
        },
        (Url::Ap(ap_url_1), Url::Ap(ap_url_2)) => {
            ap_url_1.authority() == ap_url_2.authority()
        },
        _ => false, // can't compare different types of authorities
    };
    Ok(is_same)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_url_from_http_url() {
        let url_str = "https://social.example/users/test";
        let http_url = HttpUrl::parse(url_str).unwrap();
        let url = Url::Http(http_url);
        let output = url.to_http_url(None).unwrap();
        assert_eq!(output, url_str);
    }

    #[test]
    fn test_http_url_from_ap_url() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let ap_url = ApUrl::parse(url_str).unwrap();
        let url = Url::Ap(ap_url);
        let gateway = "https://gateway.example";
        let output = url.to_http_url(Some(gateway)).unwrap();
        let expected_output = "https://gateway.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_http_url_from_ap_url_no_gateway() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let ap_url = ApUrl::parse(url_str).unwrap();
        let url = Url::Ap(ap_url);
        let maybe_output = url.to_http_url(None);
        assert!(maybe_output.is_none());
    }

    #[test]
    fn test_parse_url_https() {
        let url_str = "https://social.example/users/test";
        let (url, maybe_gateway) = parse_url(url_str).unwrap();
        assert!(matches!(url, Url::Http(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_url_i2p() {
        let url_str = "http://social.example.i2p/users/test";
        let (url, maybe_gateway) = parse_url(url_str).unwrap();
        assert!(matches!(url, Url::Http(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_url_localhost() {
        let url_str = "http://127.0.0.1:8380/users/test";
        let (url, maybe_gateway) = parse_url(url_str).unwrap();
        assert!(matches!(url, Url::Http(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_url_ap() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let (url, maybe_gateway) = parse_url(url_str).unwrap();
        assert!(matches!(url, Url::Ap(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_url_ap_with_gateway() {
        let url_str = "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let (url, maybe_gateway) = parse_url(url_str).unwrap();
        assert!(matches!(url, Url::Ap(_)));
        assert_eq!(maybe_gateway.as_deref(), Some("https://social.example"));
        let expected_canonical_url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        assert_eq!(url.to_string(), expected_canonical_url);
    }

    #[test]
    fn test_parse_url_ap_with_gateway_unsupported_did() {
        let url_str = "https://social.example/.well-known/apgateway/did:example:123456";
        let error = parse_url(url_str).err().unwrap();
        assert_eq!(error.to_string(), "invalid 'ap' URL");
    }

    #[test]
    fn test_canonicalize_id_http() {
        let url = "https://social.example/users/alice#main-key";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(canonical_url, url);

        let url = "https://social.example";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(canonical_url, url);
    }

    #[test]
    fn test_canonicalize_id_http_idn() {
        let url = "https://δοκιμή.example/users/alice#main-key";
        let result = canonicalize_id(url);
        assert!(result.is_err()); // not a URI
    }

    #[test]
    fn test_canonicalize_id_ap() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor?type=group";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(canonical_url, url);
    }

    #[test]
    fn test_canonicalize_id_gateway() {
        let url = "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key";
        let canonical_url = canonicalize_id(url).unwrap();
        assert_eq!(
            canonical_url,
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key",
        );
    }

    #[test]
    fn test_is_same_authority() {
        let id_1 = "https://one.example/1";
        let id_2 = "https://one.example/2";
        let id_3 = "https://two.example/3";
        assert_eq!(is_same_authority(id_1, id_2).unwrap(), true);
        assert_eq!(is_same_authority(id_1, id_3).unwrap(), false);

        let id_4 = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/one";
        let id_5 = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/two";
        let id_6 = "ap://did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK/one";
        assert_eq!(is_same_authority(id_4, id_5).unwrap(), true);
        assert_eq!(is_same_authority(id_4, id_6).unwrap(), false);
        assert_eq!(is_same_authority(id_4, id_1).unwrap(), false);

        let id_7 = "https://one.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        assert_eq!(is_same_authority(id_7, id_4).unwrap(), true);
        assert_eq!(is_same_authority(id_7, id_1).unwrap(), false);
    }
}
