use std::fmt;
use std::str::FromStr;

use mitra_utils::{
    ap_url::{is_ap_url, ApUrl},
    http_url::HttpUrl,
};
use mitra_validators::{
    errors::ValidationError,
};

use crate::authority::GATEWAY_PATH_PREFIX;

pub enum Url {
    Http(HttpUrl),
    Ap(ApUrl),
}

fn with_gateway(ap_url: &ApUrl, gateway_url: &str) -> String {
    format!("{}{}{}", gateway_url, GATEWAY_PATH_PREFIX, ap_url.to_did_url())
}

impl Url {
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
) -> Result<(ApUrl, String), ValidationError> {
    let relative_http_url = http_url.to_relative();
    let did_url = relative_http_url
        .strip_prefix(GATEWAY_PATH_PREFIX)
        .ok_or(ValidationError("invalid gateway URL"))?;
    let ap_url = ApUrl::from_did_url(did_url)
        .map_err(ValidationError)?;
    let gateway = http_url.origin();
    Ok((ap_url, gateway))
}

pub fn parse_url(
    value: &str,
) -> Result<(Url, Option<String>), ValidationError> {
    let mut maybe_gateway = None;
    let url = if is_ap_url(value) {
        let ap_url = ApUrl::from_str(value).map_err(ValidationError)?;
        Url::Ap(ap_url)
    } else {
        let http_url = HttpUrl::parse(value).map_err(ValidationError)?;
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
        let ap_url = url_str.parse::<ApUrl>().unwrap();
        let url = Url::Ap(ap_url);
        let gateway = "https://gateway.example";
        let output = url.to_http_url(Some(gateway)).unwrap();
        let expected_output = "https://gateway.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_http_url_from_ap_url_no_gateway() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let ap_url = url_str.parse::<ApUrl>().unwrap();
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
}
