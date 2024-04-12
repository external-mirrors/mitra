use std::fmt;
use std::str::FromStr;

use regex::Regex;

use mitra_utils::{
    did::Did,
    did_key::DidKey,
    urls::{Position, Url},
};

// https://www.w3.org/TR/did-core/
const DID_URL_PATH_RE: &str = r"^ap:(?P<method>[[:alpha:]]+):(?P<id>[A-Za-z0-9._:-]+)(?P<path>/.+)?$";

// ap:// URL must have path
const AP_URL_RE: &str = r"^ap://(?P<did>did:[[:alpha:]]+:[A-Za-z0-9._:-]+)(?P<path>/.+)$";
const AP_URL_PREFIX: &str = "ap://";

/// https://codeberg.org/fediverse/fep/src/branch/main/fep/ef61/fep-ef61.md
pub struct DidApUrl {
    did: Did,
    path: Option<String>,
}

impl DidApUrl {
    pub fn from_did_key(did_key: &DidKey) -> Self {
        let did = Did::Key(did_key.clone());
        Self { did, path: None }
    }

    pub fn did(&self) -> &Did {
        &self.did
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

impl fmt::Display for DidApUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "did:ap:{}:{}{}",
            self.did.method(),
            self.did.identifier(),
            self.path.as_deref().unwrap_or_default(),
        )
    }
}

impl FromStr for DidApUrl {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(value).map_err(|_| "invalid URL")?;
        let url_path = url.path();
        let url_path_re = Regex::new(DID_URL_PATH_RE)
             .expect("regexp should be valid");
        let captures = url_path_re.captures(url_path).ok_or("invalid URL")?;
        let did_str = format!(
            "did:{}:{}",
            &captures["method"],
            &captures["id"],
        );
        let did = Did::from_str(&did_str).map_err(|_| "invalid DID")?;
        let path = captures.name("path").map(|path| path.as_str().to_string());
        let did_url = Self { did, path };
        Ok(did_url)
    }
}

pub struct ApUrl {
    did: Did,
    url: Url,
}

impl ApUrl {
    pub fn did(&self) -> &Did {
        &self.did
    }

    pub fn relative_url(&self) -> String {
        self.url[Position::BeforePath..].to_string()
    }
}

impl fmt::Display for ApUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{}{}",
            AP_URL_PREFIX,
            self.did(),
            self.relative_url(),
        )
    }
}

impl FromStr for ApUrl {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url_re = Regex::new(AP_URL_RE)
             .expect("regexp should be valid");
        let captures = url_re.captures(value).ok_or("invalid AP URL")?;
        let did = Did::from_str(&captures["did"]).map_err(|_| "invalid DID")?;
        // Parse relative URL
        let base = Url::parse(AP_URL_PREFIX).expect("scheme should be valid");
        let url = Url::options()
            .base_url(Some(&base))
            .parse(&captures["path"])
            .map_err(|_| "invalid AP URL")?;
        if url.authority() != "" {
            return Err("invalid AP URL");
        };
        let ap_url = Self { did, url };
        Ok(ap_url)
    }
}

#[cfg(test)]
mod tests {
    use mitra_utils::{
        crypto_eddsa::{
            ed25519_public_key_from_private_key,
            generate_weak_ed25519_key,
        },
    };
    use super::*;

    #[test]
    fn test_did_url_from_key() {
        let secret_key = generate_weak_ed25519_key();
        let public_key = ed25519_public_key_from_private_key(&secret_key);
        let did_key = DidKey::from_ed25519_key(&public_key);
        let url = DidApUrl::from_did_key(&did_key);
        assert_eq!(url.did().as_did_key().unwrap(), &did_key);
        assert_eq!(url.path(), None);
    }

    #[test]
    fn test_parse_did_key_url_without_path() {
        let url_str = "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6";
        let url = DidApUrl::from_str(url_str).unwrap();
        assert_eq!(url.did().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(url.path(), None);
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_did_key_url_with_path() {
        let url_str = "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/123";
        let url = DidApUrl::from_str(url_str).unwrap();
        assert_eq!(url.did().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(url.path().unwrap(), "/objects/123");
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_ap_url() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/123";
        let url = ApUrl::from_str(url_str).unwrap();
        assert_eq!(url.did().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(url.url.scheme(), "ap");
        assert_eq!(url.url.authority(), "");
        assert_eq!(url.url.origin().is_tuple(), false);
        assert_eq!(url.relative_url(), "/objects/123");
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_ap_url_with_fragment() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key";
        let url = ApUrl::from_str(url_str).unwrap();
        assert_eq!(url.did().to_string(), "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(url.relative_url(), "/actor#main-key");
        assert_eq!(url.to_string(), url_str);
    }

    #[test]
    fn test_parse_ap_url_without_path() {
        let url_str = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6";
        let error = ApUrl::from_str(url_str).err().unwrap();
        assert_eq!(error, "invalid AP URL");
    }
}
