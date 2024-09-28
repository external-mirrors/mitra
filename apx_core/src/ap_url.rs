/// https://codeberg.org/fediverse/fep/src/branch/main/fep/ef61/fep-ef61.md
use std::fmt;
use std::str::FromStr;

use iri_string::types::UriRelativeString;
use regex::Regex;

use crate::{
    did::Did,
    url::common::{url_decode, Origin},
};

// https://www.w3.org/TR/did-core/
// ap:// URL must have path
// authority: DID regexp plus percent sign (see also: DID_RE in apx_core::did)
const AP_URL_RE: &str = r"^ap://(?P<did>did(:|%3A)[[:alpha:]]+(:|%3A)[A-Za-z0-9._:%-]+)(?P<path>/.+)$";
const AP_URL_PREFIX: &str = "ap://";

pub fn is_ap_url(url: &str) -> bool {
    url.starts_with(AP_URL_PREFIX)
}

pub fn with_ap_prefix(did_url: &str) -> String {
    format!("{}{}", AP_URL_PREFIX, did_url)
}

#[derive(Clone)]
pub struct ApUrl {
    authority: Did,
    location: UriRelativeString,
}

impl ApUrl {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let url_re = Regex::new(AP_URL_RE)
             .expect("regexp should be valid");
        let captures = url_re.captures(value).ok_or("invalid 'ap' URL")?;
        let did_str = url_decode(&captures["did"]);
        let authority = Did::from_str(&did_str)
            .map_err(|_| "invalid 'ap' URL authority")?;
        // Authority should be an Ed25519 key
        if authority.as_did_key()
            .and_then(|did_key| did_key.try_ed25519_key().ok())
            .is_none()
        {
            return Err("invalid 'ap' URL authority");
        };
        // Parse relative URL
        let location = UriRelativeString::from_str(&captures["path"])
            .map_err(|_| "invalid 'ap' URL")?;
        if location.authority_str().is_some() {
            return Err("invalid 'ap' URL");
        };
        let ap_url = Self { authority, location };
        Ok(ap_url)
    }

    pub fn authority(&self) -> &Did {
        &self.authority
    }

    pub fn relative_url(&self) -> String {
        format!(
            "{}{}{}",
            self.location.path_str(),
            self.location.query().map(|query| format!("?{query}")).unwrap_or_default(),
            self.location.fragment().map(|frag| format!("#{frag}")).unwrap_or_default(),
        )
    }

    pub fn from_did_url(did_url: &str) -> Result<Self, &'static str> {
        Self::parse(&with_ap_prefix(did_url))
    }

    pub fn to_did_url(&self) -> String {
        format!("{}{}", self.authority(), self.relative_url())
    }

    // https://www.rfc-editor.org/rfc/rfc6454.html
    pub fn origin(&self) -> Origin {
        // Default port is 0
        Origin::new("ap", &self.authority.to_string(), 0)
    }
}

impl fmt::Display for ApUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{}",
            AP_URL_PREFIX,
            self.to_did_url(),
        )
    }
}

impl FromStr for ApUrl {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ap_url() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/123";
        let url = ApUrl::parse(url_str).unwrap();
        assert_eq!(url.authority().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(url.location.authority_str(), None);
        assert_eq!(url.location.path_str(), "/objects/123");
        assert_eq!(url.relative_url(), "/objects/123");
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_ap_url_with_fragment() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key";
        let url = ApUrl::parse(url_str).unwrap();
        assert_eq!(url.authority().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(url.relative_url(), "/actor#main-key");
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_ap_url_with_percent_encoded_authority() {
        let url_str = "ap://did%3Akey%3Az6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2/actor";
        let url = ApUrl::parse(url_str).unwrap();
        assert_eq!(url.authority().to_string(), "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2");
    }

    #[test]
    fn test_parse_ap_url_without_path() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6";
        let error = ApUrl::parse(url_str).err().unwrap();
        assert_eq!(error, "invalid 'ap' URL");
    }

    #[test]
    fn test_parse_ap_url_empty_path() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/";
        let error = ApUrl::parse(url_str).err().unwrap();
        assert_eq!(error, "invalid 'ap' URL");

        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6//";
        let error = ApUrl::parse(url_str).err().unwrap();
        assert_eq!(error, "invalid 'ap' URL");
    }

    #[test]
    fn test_parse_ap_url_with_double_slash() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6//actor";
        let error = ApUrl::parse(url_str).err().unwrap();
        assert_eq!(error, "invalid 'ap' URL");
    }

    #[test]
    fn test_origin() {
        let ap_url = ApUrl::parse("ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor").unwrap();
        assert_eq!(ap_url.origin(), Origin::new("ap", "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6", 0));
    }
}
