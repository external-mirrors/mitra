use std::fmt;

use apx_core::{
    ap_url::with_ap_prefix,
    crypto_eddsa::{
        ed25519_public_key_from_secret_key,
        Ed25519SecretKey,
        Ed25519PublicKey,
    },
    did_key::DidKey,
    url::canonical::GATEWAY_PATH_PREFIX,
};

use mitra_config::Instance;
use mitra_models::users::types::User;

fn fep_ef61_identity(public_key: &Ed25519PublicKey) -> DidKey {
    DidKey::from_ed25519_key(public_key)
}

pub enum Authority {
    Server(String),
    // TODO: FEP-EF61: remove server URL after transition
    Key((String, Ed25519PublicKey)),
    KeyWithGateway((String, Ed25519PublicKey)),
}

impl fmt::Display for Authority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let authority_str = match self {
            Self::Server(ref server_url) => server_url.to_owned(),
            Self::Key((_, public_key)) => {
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
    pub fn server(server_url: &str) -> Self {
        Self::Server(server_url.to_owned())
    }

    #[allow(dead_code)]
    fn key(server_url: &str, secret_key: &Ed25519SecretKey) -> Self {
        let public_key = ed25519_public_key_from_secret_key(secret_key);
        Self::Key((server_url.to_owned(), public_key))
    }

    fn key_with_gateway(server_url: &str, secret_key: &Ed25519SecretKey) -> Self {
        let public_key = ed25519_public_key_from_secret_key(secret_key);
        Self::KeyWithGateway((server_url.to_owned(), public_key))
    }

    pub fn from_user(
        server_url: &str,
        user: &User,
        fep_ef61_enabled: bool,
    ) -> Self {
        if fep_ef61_enabled {
            Self::key_with_gateway(server_url, &user.ed25519_secret_key)
        } else {
            Self::server(server_url)
        }
    }

    pub fn is_fep_ef61(&self) -> bool {
        !matches!(self, Self::Server(_))
    }

    pub fn server_url(&self) -> &str {
        match self {
            Self::Server(ref server_url) => server_url,
            Self::Key((ref server_url, _)) => server_url,
            Self::KeyWithGateway((ref server_url, _)) => server_url,
        }
    }

    pub fn as_did_key(&self) -> Option<DidKey> {
        match self {
            Self::Server(_) => None,
            Self::Key((_, public_key)) | Self::KeyWithGateway((_, public_key)) => {
                Some(fep_ef61_identity(public_key))
            },
        }
    }
}

impl From<&Instance> for Authority {
    fn from(instance: &Instance) -> Self {
        Self::server(&instance.url())
    }
}

#[cfg(test)]
mod tests {
    use apx_core::crypto_eddsa::generate_weak_ed25519_key;
    use super::*;

    const SERVER_URL: &str = "https://server.example";

    #[test]
    fn test_authority_server() {
        let authority = Authority::server(SERVER_URL);
        assert!(!authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example");
        assert_eq!(authority.server_url(), SERVER_URL);
        assert_eq!(authority.as_did_key().is_none(), true);
    }

    #[test]
    fn test_authority_key() {
        let secret_key = generate_weak_ed25519_key();
        let authority = Authority::key(SERVER_URL, &secret_key);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.server_url(), SERVER_URL);
        assert_eq!(authority.as_did_key().is_some(), true);
    }

    #[test]
    fn test_authority_key_with_gateway() {
        let secret_key = generate_weak_ed25519_key();
        let authority = Authority::key_with_gateway(SERVER_URL, &secret_key);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.server_url(), SERVER_URL);
        assert_eq!(authority.as_did_key().is_some(), true);
    }
}
