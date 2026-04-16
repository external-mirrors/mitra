use std::fmt;

use apx_core::{
    crypto::eddsa::{
        ed25519_public_key_from_secret_key,
        Ed25519SecretKey,
        Ed25519PublicKey,
    },
    did_key::DidKey,
    url::{
        ap_uri::with_ap_prefix,
        canonical::GATEWAY_PATH_PREFIX,
        http_uri::HttpUri,
    },
};

use mitra_config::Instance;

fn fep_ef61_identity(public_key: &Ed25519PublicKey) -> DidKey {
    DidKey::from_ed25519_key(public_key)
}

#[derive(Clone)]
pub enum AuthorityRoot {
    Server(HttpUri),
    Key(Ed25519PublicKey),
}

impl fmt::Display for AuthorityRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base_uri = match self {
            AuthorityRoot::Server(server_url) => server_url.to_string(),
            AuthorityRoot::Key(public_key) => {
                let did = fep_ef61_identity(public_key);
                with_ap_prefix(&did.to_string())
            },
        };
        write!(formatter, "{}", base_uri)
    }
}

// Local naming authority
pub struct Authority {
    // Authority for IDs
    // TODO: add webfinger_hostname?
    root: AuthorityRoot,
    // FEP-ef61 ID generation options
    http_base_uri: Option<HttpUri>, // TODO: multiple gateways
    prefer_compatible: bool,
}

impl fmt::Display for Authority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base_uri = match self.root {
            AuthorityRoot::Server(_) => self.root.to_string(),
            AuthorityRoot::Key(ref public_key) => {
                match self.http_base_uri {
                    Some(ref http_base_uri) if self.prefer_compatible => {
                        let did = fep_ef61_identity(public_key);
                        format!(
                            "{}{}{}",
                            http_base_uri,
                            GATEWAY_PATH_PREFIX,
                            did,
                        )
                    },
                    _ => self.root.to_string(),
                }
            },
        };
        write!(formatter, "{}", base_uri)
    }
}

impl Authority {
    pub fn server(server_uri: &HttpUri) -> Self {
        let root = AuthorityRoot::Server(server_uri.clone());
        Self {
            root,
            http_base_uri: Some(server_uri.clone()),
            prefer_compatible: true,
        }
    }

    // TODO: remove
    pub fn server_unchecked(server_uri: &str) -> Self {
        let server_uri = HttpUri::parse(server_uri)
            .expect("server URI should be valid");
        Self::server(&server_uri)
    }

    // TODO: make public after removing expect_server_uri()
    #[allow(dead_code)]
    fn key(secret_key: &Ed25519SecretKey) -> Self {
        let public_key = ed25519_public_key_from_secret_key(secret_key);
        let root = AuthorityRoot::Key(public_key);
        Self {
            root,
            http_base_uri: None,
            prefer_compatible: true,
        }
    }

    pub fn key_with_gateway(secret_key: &Ed25519SecretKey, server_uri: &HttpUri) -> Self {
        let public_key = ed25519_public_key_from_secret_key(secret_key);
        let root = AuthorityRoot::Key(public_key);
        Self {
            root,
            http_base_uri: Some(server_uri.clone()),
            prefer_compatible: true,
        }
    }

    pub fn and_prefer_canonical(&self) -> Self {
        Self {
            root: self.root.clone(),
            http_base_uri: self.http_base_uri.clone(),
            prefer_compatible: false,
        }
    }

    pub fn root(&self) -> &AuthorityRoot {
        &self.root
    }

    pub fn is_fep_ef61(&self) -> bool {
        !matches!(self.root, AuthorityRoot::Server(_))
    }

    // TODO: remove
    /// Returns web server base URI
    pub fn server_uri(&self) -> Option<&HttpUri> {
        match self.root {
            AuthorityRoot::Server(ref server_uri) => Some(server_uri),
            AuthorityRoot::Key(_) => self.http_base_uri.as_ref(),
        }
    }

    // TODO: remove
    pub fn expect_server_uri(&self) -> &HttpUri {
        self.server_uri().expect("authority should be anchored")
    }

    // TODO: remove
    pub fn as_did_key(&self) -> Option<DidKey> {
        match self.root {
            AuthorityRoot::Server(_) => None,
            AuthorityRoot::Key(ref public_key) => {
                Some(fep_ef61_identity(public_key))
            },
        }
    }
}

#[cfg(not(feature = "mini"))]
impl From<&Instance> for Authority {
    fn from(instance: &Instance) -> Self {
        Self::server(instance.uri())
    }
}

#[cfg(feature = "mini")]
impl From<&Instance> for Authority {
    fn from(instance: &Instance) -> Self {
        let gateway_uri = instance.uri();
        Self::key_with_gateway(&instance.ed25519_secret_key, gateway_uri)
    }
}

#[cfg(test)]
mod tests {
    use apx_core::crypto::eddsa::generate_weak_ed25519_key;
    use super::*;

    const SERVER_URI: &str = "https://server.example";

    #[test]
    fn test_authority_server() {
        let server_uri = HttpUri::parse(SERVER_URI).unwrap();
        let authority = Authority::server(&server_uri);
        assert!(!authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example");
        assert_eq!(authority.server_uri().unwrap().as_str(), SERVER_URI);
        assert_eq!(authority.as_did_key().is_none(), true);
    }

    #[test]
    fn test_authority_key() {
        let secret_key = generate_weak_ed25519_key();
        let authority = Authority::key(&secret_key);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.server_uri(), None);
        assert_eq!(authority.as_did_key().is_some(), true);
    }

    #[test]
    fn test_authority_key_with_gateway() {
        let secret_key = generate_weak_ed25519_key();
        let server_uri = HttpUri::parse(SERVER_URI).unwrap();
        let authority = Authority::key_with_gateway(&secret_key, &server_uri);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.server_uri().unwrap().as_str(), SERVER_URI);
        assert_eq!(authority.as_did_key().is_some(), true);
    }

    #[test]
    fn test_authority_key_with_gateway_prefer_canonical() {
        let secret_key = generate_weak_ed25519_key();
        let server_uri = HttpUri::parse(SERVER_URI).unwrap();
        let mut authority = Authority::key_with_gateway(&secret_key, &server_uri);
        authority.prefer_compatible = false;
        assert_eq!(authority.to_string(), "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
    }
}
