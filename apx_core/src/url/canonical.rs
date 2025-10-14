//! Canonical URI

use std::fmt;

use serde::{
    Deserialize,
    Deserializer,
    de::Error as DeserializerError,
};
use thiserror::Error;

use crate::{
    ap_url::{is_ap_url as is_ap_uri, ApUrl as ApUri},
    http_url::{HttpUrl as HttpUri},
    url::common::Origin,
};

pub const GATEWAY_PATH_PREFIX: &str = "/.well-known/apgateway/";

#[derive(Debug, Error)]
#[error("{0}")]
pub struct CanonicalUriError(pub &'static str);

#[derive(Clone, PartialEq)]
pub enum CanonicalUri {
    Http(HttpUri),
    Ap(ApUri),
}

pub fn with_gateway(ap_uri: &ApUri, gateway_base: &str) -> String {
    format!("{}{}{}", gateway_base, GATEWAY_PATH_PREFIX, ap_uri.to_did_url())
}

impl CanonicalUri {
    pub fn parse(value: &str) -> Result<Self, CanonicalUriError> {
        let (canonical_uri, _) = parse_url(value)?;
        Ok(canonical_uri)
    }

    pub fn parse_canonical(value: &str) -> Result<Self, CanonicalUriError> {
        let (canonical_uri, maybe_gateway) = parse_url(value)?;
        if maybe_gateway.is_some() {
            return Err(CanonicalUriError("URI is not canonical"));
        };
        Ok(canonical_uri)
    }

    pub fn authority(&self) -> String {
        match self {
            Self::Http(http_uri) => http_uri.authority().to_string(),
            Self::Ap(ap_uri) => ap_uri.authority().to_string(),
        }
    }

    pub fn origin(&self) -> Origin {
        match self {
            Self::Http(http_uri) => http_uri.origin(),
            Self::Ap(ap_uri) => ap_uri.origin(),
        }
    }

    pub fn to_http_uri(&self, maybe_gateway: Option<&str>) -> Option<String> {
        let http_uri = match self {
            Self::Http(http_uri) => http_uri.to_string(),
            Self::Ap(ap_uri) => {
                if let Some(gateway) = maybe_gateway {
                    with_gateway(ap_uri, gateway)
                } else {
                    // Not enough context
                    return None;
                }
            },
        };
        Some(http_uri)
    }
}

impl fmt::Display for CanonicalUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(http_uri) => write!(formatter, "{}", http_uri),
            Self::Ap(ap_uri) => write!(formatter, "{}", ap_uri),
        }
    }
}

fn get_canonical_ap_uri(
    http_uri: HttpUri,
) -> Result<(ApUri, String), CanonicalUriError> {
    let relative_http_uri = http_uri.to_relative();
    let did_url = relative_http_uri
        .strip_prefix(GATEWAY_PATH_PREFIX)
        .ok_or(CanonicalUriError("invalid gateway URI"))?;
    let ap_uri = ApUri::from_did_url(did_url)
        .map_err(CanonicalUriError)?;
    let gateway = http_uri.base();
    Ok((ap_uri, gateway))
}

pub fn parse_url(
    value: &str,
) -> Result<(CanonicalUri, Option<String>), CanonicalUriError> {
    let mut maybe_gateway = None;
    let canonical_uri = if is_ap_uri(value) {
        let ap_uri = ApUri::parse(value).map_err(CanonicalUriError)?;
        CanonicalUri::Ap(ap_uri)
    } else {
        let http_uri = HttpUri::parse(value).map_err(CanonicalUriError)?;
        if http_uri.path().starts_with(GATEWAY_PATH_PREFIX) {
            let (ap_uri, gateway) = get_canonical_ap_uri(http_uri)?;
            maybe_gateway = Some(gateway);
            CanonicalUri::Ap(ap_uri)
        } else {
            CanonicalUri::Http(http_uri)
        }
    };
    Ok((canonical_uri, maybe_gateway))
}

impl<'de> Deserialize<'de> for CanonicalUri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let value: String = Deserialize::deserialize(deserializer)?;
        Self::parse(&value).map_err(DeserializerError::custom)
    }
}

pub fn is_same_origin(id_1: &str, id_2: &str) -> Result<bool, CanonicalUriError> {
    let id_1 = CanonicalUri::parse(id_1)?;
    let id_2 = CanonicalUri::parse(id_2)?;
    let is_same = id_1.origin() == id_2.origin();
    Ok(is_same)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_uri_from_http_uri() {
        let input = "https://social.example/users/test";
        let http_uri = HttpUri::parse(input).unwrap();
        let canonical_uri = CanonicalUri::Http(http_uri);
        let output = canonical_uri.to_http_uri(None).unwrap();
        assert_eq!(output, input);
    }

    #[test]
    fn test_http_uri_from_ap_uri() {
        let input = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let ap_uri = ApUri::parse(input).unwrap();
        let canonical_uri = CanonicalUri::Ap(ap_uri);
        let gateway = "https://gateway.example";
        let output = canonical_uri.to_http_uri(Some(gateway)).unwrap();
        let expected_output = "https://gateway.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_http_uri_from_ap_uri_no_gateway() {
        let input = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let ap_uri = ApUri::parse(input).unwrap();
        let canonical_uri = CanonicalUri::Ap(ap_uri);
        let maybe_output = canonical_uri.to_http_uri(None);
        assert!(maybe_output.is_none());
    }

    #[test]
    fn test_parse_url_https() {
        let url = "https://social.example/users/test";
        let (canonical_uri, maybe_gateway) = parse_url(url).unwrap();
        assert!(matches!(canonical_uri, CanonicalUri::Http(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(canonical_uri.to_string(), url);
    }

    #[test]
    fn test_parse_url_https_with_fragment() {
        let url = "https://www.w3.org/ns/activitystreams#Public";
        let (canonical_uri, maybe_gateway) = parse_url(url).unwrap();
        assert!(matches!(canonical_uri, CanonicalUri::Http(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(canonical_uri.to_string(), url);
    }

    #[test]
    fn test_parse_url_i2p() {
        let url = "http://social.example.i2p/users/test";
        let (canonical_uri, maybe_gateway) = parse_url(url).unwrap();
        assert!(matches!(canonical_uri, CanonicalUri::Http(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(canonical_uri.to_string(), url);
    }

    #[test]
    fn test_parse_url_localhost() {
        let url = "http://127.0.0.1:8380/users/test";
        let (canonical_uri, maybe_gateway) = parse_url(url).unwrap();
        assert!(matches!(canonical_uri, CanonicalUri::Http(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(canonical_uri.to_string(), url);
    }

    #[test]
    fn test_parse_url_ap() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let (canonical_uri, maybe_gateway) = parse_url(url).unwrap();
        assert!(matches!(canonical_uri, CanonicalUri::Ap(_)));
        assert_eq!(maybe_gateway, None);
        assert_eq!(canonical_uri.to_string(), url);
    }

    #[test]
    fn test_parse_url_ap_with_gateway() {
        let url = "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let (canonical_uri, maybe_gateway) = parse_url(url).unwrap();
        assert!(matches!(canonical_uri, CanonicalUri::Ap(_)));
        assert_eq!(maybe_gateway.as_deref(), Some("https://social.example"));
        let expected_canonical_uri = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        assert_eq!(canonical_uri.to_string(), expected_canonical_uri);
    }

    #[test]
    fn test_parse_url_ap_with_gateway_unsupported_did() {
        let url = "https://social.example/.well-known/apgateway/did:example:123456";
        let error = parse_url(url).err().unwrap();
        assert_eq!(error.to_string(), "invalid 'ap' URL");
    }

    #[test]
    fn test_is_same_origin() {
        let id_1 = "https://one.example/1";
        let id_2 = "https://one.example/2";
        let id_3 = "https://two.example/3";
        assert_eq!(is_same_origin(id_1, id_2).unwrap(), true);
        assert_eq!(is_same_origin(id_1, id_3).unwrap(), false);

        let id_4 = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/one";
        let id_5 = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/two";
        let id_6 = "ap://did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK/one";
        assert_eq!(is_same_origin(id_4, id_5).unwrap(), true);
        assert_eq!(is_same_origin(id_4, id_6).unwrap(), false);
        assert_eq!(is_same_origin(id_4, id_1).unwrap(), false);

        let id_7 = "https://one.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        assert_eq!(is_same_origin(id_7, id_4).unwrap(), true);
        assert_eq!(is_same_origin(id_7, id_1).unwrap(), false);
    }
}
