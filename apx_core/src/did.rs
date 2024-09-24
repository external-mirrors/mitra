/// https://www.w3.org/TR/did-core/
use std::fmt;
use std::str::FromStr;

use iri_string::types::UriRelativeString;
use regex::Regex;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeserializerError,
};

use crate::{
    did_key::DidKey,
    did_pkh::DidPkh,
};

// https://www.w3.org/TR/did-core/#did-syntax
const DID_RE: &str = r"^did:(?P<method>[[:alpha:]]+):[A-Za-z0-9._:-]+$";
// https://www.w3.org/TR/did-core/#did-url-syntax
const DID_URL_RE: &str = r"^(?P<did>did:[[:alpha:]]+:[A-Za-z0-9._:-]+)(?P<resource>.*)$";

#[derive(Clone, Debug, PartialEq)]
pub enum Did {
    Key(DidKey),
    Pkh(DidPkh),
}

#[derive(thiserror::Error, Debug)]
#[error("DID parse error")]
pub struct DidParseError;

impl Did {
    /// Parse DID URL
    pub fn parse_url(url: &str) -> Result<(Self, UriRelativeString), DidParseError> {
        let url_re = Regex::new(DID_URL_RE)
            .expect("regexp should be valid");
        let captures = url_re.captures(url).ok_or(DidParseError)?;
        let did = Did::from_str(&captures["did"])
            .map_err(|_| DidParseError)?;
        let resource = UriRelativeString::from_str(&captures["resource"])
            .map_err(|_| DidParseError)?;
        Ok((did, resource))
    }

    pub fn method(&self) -> &'static str {
        match self {
            Did::Key(_) => DidKey::METHOD,
            Did::Pkh(_) => DidPkh::METHOD,
        }
    }

    pub fn identifier(&self) -> String {
        match self {
            Did::Key(did_key) => did_key.key_multibase(),
            Did::Pkh(did_pkh) => did_pkh.account_id().to_string(),
        }
    }

    pub fn as_did_key(&self) -> Option<&DidKey> {
        match self {
            Did::Key(did_key) => Some(did_key),
            _ => None,
        }
    }

    pub fn as_did_pkh(&self) -> Option<&DidPkh> {
        match self {
            Did::Pkh(did_pkh) => Some(did_pkh),
            _ => None,
        }
    }
}

impl FromStr for Did {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let did_re = Regex::new(DID_RE).expect("regexp should be valid");
        let caps = did_re.captures(value).ok_or(DidParseError)?;
        let did = match &caps["method"] {
            "key" => {
                let did_key = DidKey::from_str(value)?;
                Self::Key(did_key)
            },
            "pkh" => {
                let did_pkh = DidPkh::from_str(value)?;
                Self::Pkh(did_pkh)
            },
            _ => return Err(DidParseError),
        };
        Ok(did)
    }
}

impl fmt::Display for Did {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let did_str = match self {
            Self::Key(did_key) => did_key.to_string(),
            Self::Pkh(did_pkh) => did_pkh.to_string(),
        };
        write!(formatter, "{}", did_str)
    }
}

impl<'de> Deserialize<'de> for Did {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let did_str: String = Deserialize::deserialize(deserializer)?;
        did_str.parse().map_err(DeserializerError::custom)
    }
}

impl Serialize for Did {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        let did_str = self.to_string();
        serializer.serialize_str(&did_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_key_string_conversion() {
        let did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let did: Did = did_str.parse().unwrap();
        assert!(matches!(did, Did::Key(_)));
        assert_eq!(did.method(), "key");
        assert_eq!(did.identifier(), "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
        assert_eq!(did.to_string(), did_str);
    }

    #[test]
    fn test_did_pkh_string_conversion() {
        let did_str = "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a";
        let did: Did = did_str.parse().unwrap();
        assert!(matches!(did, Did::Pkh(_)));
        assert_eq!(did.method(), "pkh");
        assert_eq!(did.identifier(), "eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a");
        assert_eq!(did.to_string(), did_str);
    }

    #[test]
    fn test_did_parse_http_url() {
        let value = "https://social.example/resolver/did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let result = value.parse::<Did>();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_did_url() {
        let did_url_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK#z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let (did, resource) = Did::parse_url(did_url_str).unwrap();
        assert_eq!(did.method(), "key");
        assert_eq!(did.identifier(), "z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
        assert_eq!(resource.path_str(), "");
        assert_eq!(resource.query_str(), None);
        assert_eq!(
            resource.fragment().map(|fragment| fragment.as_str()),
            Some("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"),
        );
    }
}
