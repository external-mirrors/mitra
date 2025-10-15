use std::fmt;

use apx_core::{
    crypto_eddsa::{
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

pub enum Authority {
    Server(HttpUri),
    Key(Ed25519PublicKey),
    KeyWithGateway((HttpUri, Ed25519PublicKey)),
}

impl fmt::Display for Authority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let authority_str = match self {
            Self::Server(server_url) => server_url.to_string(),
            Self::Key(public_key) => {
                let did = fep_ef61_identity(public_key);
                with_ap_prefix(&did.to_string())
            },
            Self::KeyWithGateway((server_url, public_key)) => {
                let did = fep_ef61_identity(public_key);
                format!(
                    "{}{}{}",
                    server_url,
                    GATEWAY_PATH_PREFIX,
                    did,
                )
            },
        };
        write!(formatter, "{}", authority_str)
    }
}

impl Authority {
    pub fn server(server_uri: &HttpUri) -> Self {
        Self::Server(server_uri.clone())
    }

    #[allow(dead_code)]
    fn key(secret_key: &Ed25519SecretKey) -> Self {
        let public_key = ed25519_public_key_from_secret_key(secret_key);
        Self::Key(public_key)
    }

    pub fn key_with_gateway(server_uri: &HttpUri, secret_key: &Ed25519SecretKey) -> Self {
        let public_key = ed25519_public_key_from_secret_key(secret_key);
        Self::KeyWithGateway((server_uri.clone(), public_key))
    }

    pub fn is_fep_ef61(&self) -> bool {
        !matches!(self, Self::Server(_))
    }

    pub fn server_uri(&self) -> Option<&str> {
        match self {
            Self::Server(server_uri) => Some(server_uri.as_str()),
            Self::Key(_) => None,
            Self::KeyWithGateway((server_uri, _)) => Some(server_uri.as_str()),
        }
    }

    pub fn as_did_key(&self) -> Option<DidKey> {
        match self {
            Self::Server(_) => None,
            Self::Key(public_key) | Self::KeyWithGateway((_, public_key)) => {
                Some(fep_ef61_identity(public_key))
            },
        }
    }
}

impl From<&Instance> for Authority {
    fn from(instance: &Instance) -> Self {
        Self::server(instance.uri())
    }
}

#[cfg(test)]
mod tests {
    use apx_core::crypto_eddsa::generate_weak_ed25519_key;
    use super::*;

    const SERVER_URI: &str = "https://server.example";

    #[test]
    fn test_authority_server() {
        let server_uri = HttpUri::parse(SERVER_URI).unwrap();
        let authority = Authority::server(&server_uri);
        assert!(!authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example");
        assert_eq!(authority.server_uri().unwrap(), SERVER_URI);
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
        let authority = Authority::key_with_gateway(&server_uri, &secret_key);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.server_uri().unwrap(), SERVER_URI);
        assert_eq!(authority.as_did_key().is_some(), true);
    }
}
