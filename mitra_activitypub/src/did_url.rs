use std::fmt;
use std::str::FromStr;

use regex::Regex;

use mitra_utils::{
    did::Did,
    urls::{Position, Url},
};

// https://www.w3.org/TR/did-core/
// ap:// URL must have path
const AP_URL_RE: &str = r"^ap://(?P<did>did:[[:alpha:]]+:[A-Za-z0-9._:-]+)(?P<path>/.+)$";
const AP_URL_PREFIX: &str = "ap://";

pub fn with_ap_prefix(did_url: &str) -> String {
    format!("{}{}", AP_URL_PREFIX, did_url)
}

/// https://codeberg.org/fediverse/fep/src/branch/main/fep/ef61/fep-ef61.md
pub struct ApUrl {
    did: Did,
    url: Url,
}

impl ApUrl {
    pub fn from_did_url(did_url: &str) -> Result<Self, &'static str> {
        Self::from_str(&with_ap_prefix(did_url))
    }

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
    use super::*;

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
