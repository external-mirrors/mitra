//! 'ap' URIs
//!
//! <https://codeberg.org/fediverse/fep/src/branch/main/fep/ef61/fep-ef61.md>
use std::fmt;
use std::str::FromStr;

use iri_string::types::UriRelativeString;
use regex::Regex;

use crate::{
    did::Did,
    url::common::{url_decode, Origin},
};

// https://www.w3.org/TR/did-core/
// 'ap' URI must have path
// authority: DID regexp plus percent sign (see also: DID_RE in apx_core::did)
const AP_URI_RE: &str = r"^ap://(?P<did>did(:|%3A)[[:alpha:]]+(:|%3A)[A-Za-z0-9._:%-]+)(?P<path>/.+)$";
const AP_URI_PREFIX: &str = "ap://";

pub fn is_ap_uri(uri: &str) -> bool {
    uri.starts_with(AP_URI_PREFIX)
}

pub fn with_ap_prefix(did_url: &str) -> String {
    format!("{}{}", AP_URI_PREFIX, did_url)
}

// Removes query parameters from relative URI
fn remove_query(uri: UriRelativeString) -> UriRelativeString {
    let without_query = format!(
        "{}{}",
        uri.path_str(),
        uri.fragment().map(|frag| format!("#{frag}")).unwrap_or_default(),
    );
    UriRelativeString::from_str(&without_query)
        .expect("URI should be valid")
}

/// FEP-ef61 'ap' URI
#[derive(Clone, Debug, PartialEq)]
pub struct ApUri {
    authority: Did,
    location: UriRelativeString,
}

impl ApUri {
    /// Parses 'ap' URI.
    ///
    /// Query parameters are preserved.
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let uri_re = Regex::new(AP_URI_RE)
             .expect("regexp should be valid");
        let captures = uri_re.captures(value).ok_or("invalid 'ap' URI")?;
        let did_str = url_decode(&captures["did"]);
        let authority = Did::from_str(&did_str)
            .map_err(|_| "invalid 'ap' URI authority")?;
        // Authority should be an Ed25519 key
        if authority.as_did_key()
            .and_then(|did_key| did_key.try_ed25519_key().ok())
            .is_none()
        {
            return Err("invalid 'ap' URI authority");
        };
        // Parse relative URI
        let location = UriRelativeString::from_str(&captures["path"])
            .map_err(|_| "invalid 'ap' URI")?;
        if location.authority_str().is_some() {
            return Err("invalid 'ap' URI");
        };
        let ap_uri = Self { authority, location };
        Ok(ap_uri)
    }

    pub fn authority(&self) -> &Did {
        &self.authority
    }

    pub fn relative_uri(&self) -> String {
        self.location.to_string()
    }

    pub fn from_did_url(did_url: &str) -> Result<Self, &'static str> {
        Self::parse(&with_ap_prefix(did_url))
    }

    pub fn to_did_url(&self) -> String {
        format!("{}{}", self.authority(), self.relative_uri())
    }

    /// Returns origin tuple for this URI
    pub fn origin(&self) -> Origin {
        self.authority.origin()
    }

    /// Returns this URI, but without the query component
    pub fn without_query(&self) -> Self {
        let mut cloned = self.clone();
        cloned.location = remove_query(cloned.location);
        cloned
    }

    /// Returns URI without the fragment part
    pub fn without_fragment(&self) -> Self {
        let mut cloned = self.clone();
        cloned.location.set_fragment(None);
        cloned
    }
}

impl fmt::Display for ApUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{}",
            AP_URI_PREFIX,
            self.to_did_url(),
        )
    }
}

impl FromStr for ApUri {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/123";
        let ap_uri = ApUri::parse(url).unwrap();
        assert_eq!(ap_uri.authority().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(ap_uri.location.authority_str(), None);
        assert_eq!(ap_uri.location.path_str(), "/objects/123");
        assert_eq!(ap_uri.relative_uri(), "/objects/123");
        assert_eq!(ap_uri.to_string(), url);
    }

    #[test]
    fn test_parse_with_query() {
        let url = "ap://did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2/actor?gateways=https%3A%2F%2Fserver1.example,https%3A%2F%2Fserver2.example";
        let ap_uri = ApUri::parse(url).unwrap();
        assert_eq!(ap_uri.relative_uri(), "/actor?gateways=https%3A%2F%2Fserver1.example,https%3A%2F%2Fserver2.example");
        assert_eq!(ap_uri.to_string(), url);
    }

    #[test]
    fn test_parse_with_fragment() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key";
        let ap_uri = ApUri::parse(url).unwrap();
        assert_eq!(ap_uri.authority().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(ap_uri.relative_uri(), "/actor#main-key");
        assert_eq!(ap_uri.to_string(), url);
    }

    #[test]
    fn test_parse_percent_encoded_authority() {
        let url = "ap://did%3Akey%3Az6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2/actor";
        let ap_uri = ApUri::parse(url).unwrap();
        assert_eq!(ap_uri.authority().to_string(), "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2");
    }

    #[test]
    fn test_parse_without_path() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6";
        let error = ApUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid 'ap' URI");
    }

    #[test]
    fn test_parse_with_empty_path() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/";
        let error = ApUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid 'ap' URI");

        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6//";
        let error = ApUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid 'ap' URI");
    }

    #[test]
    fn test_parse_with_double_slash() {
        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6//actor";
        let error = ApUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid 'ap' URI");
    }

    #[test]
    fn test_origin() {
        let ap_uri = ApUri::parse("ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor").unwrap();
        assert_eq!(
            ap_uri.origin(),
            Origin::new("ap", "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6", 0),
        );
    }

    #[test]
    fn test_without_query() {
        let url = "ap://did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2/actor?gateways=https%3A%2F%2Fserver1.example,https%3A%2F%2Fserver2.example";
        let ap_uri = ApUri::parse(url).unwrap();
        let ap_uri_without_query = ap_uri.without_query();
        assert_eq!(ap_uri_without_query.relative_uri(), "/actor");
        assert_eq!(ap_uri_without_query.to_string(), "ap://did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2/actor");
    }

    #[test]
    fn test_without_fragment() {
        let ap_uri = ApUri::parse("ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key").unwrap();
        assert_eq!(
            ap_uri.without_fragment().to_string(),
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
        );
    }
}
