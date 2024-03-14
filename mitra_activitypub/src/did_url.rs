use std::fmt;
use std::str::FromStr;

use regex::Regex;

use mitra_utils::{
    did::Did,
    did_key::DidKey,
    urls::Url,
};

// https://www.w3.org/TR/did-core/
const DID_URL_PATH_RE: &str = r"^ap:(?P<method>[[:alpha:]]+):(?P<id>[A-Za-z0-9._:-]+)(?P<path>/.+)?$";

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

    #[allow(dead_code)]
    pub fn did(&self) -> &Did {
        &self.did
    }

    #[allow(dead_code)]
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
        let did_key = DidKey::from_ed25519_key(public_key.as_bytes());
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
}
