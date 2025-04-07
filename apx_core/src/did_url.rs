//! DID URLs
use std::fmt;
use std::str::FromStr;

use iri_string::types::UriRelativeString;
use regex::Regex;

use super::{
    did::{Did, DID_URL_RE},
    url::common::Origin,
};

/// DID URL
#[derive(Debug, PartialEq)]
pub struct DidUrl {
    did: Did,
    resource: UriRelativeString,
}

impl DidUrl {
    /// Parses DID URL
    pub fn parse(url: &str) -> Result<Self, &'static str> {
        let url_re = Regex::new(DID_URL_RE)
            .expect("regexp should be valid");
        let captures = url_re.captures(url).ok_or("invalid DID URL")?;
        let did = Did::from_str(&captures["did"])
            .map_err(|_| "invalid DID")?;
        let resource = UriRelativeString::from_str(&captures["resource"])
            .map_err(|_| "invalid DID URL")?;
        let did_url = Self { did, resource };
        Ok(did_url)
    }

    /// Returns DID of this URL
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Returns relative resource identifier
    pub fn resource(&self) -> &str {
        self.resource.as_str()
    }

    /// Returns origin tuple for this URL
    pub fn origin(&self) -> Origin {
        self.did.origin()
    }
}

impl fmt::Display for DidUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.did, self.resource)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_did_url() {
        let did_url_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK#z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let did_url = DidUrl::parse(did_url_str).unwrap();
        assert_eq!(did_url.did.method(), "key");
        assert_eq!(did_url.did.identifier(), "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
        assert_eq!(did_url.resource.path_str(), "");
        assert_eq!(did_url.resource.query_str(), None);
        assert_eq!(
            did_url.resource.fragment_str(),
            Some("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"),
        );
        assert_eq!(did_url.to_string(), did_url_str);
    }
}
