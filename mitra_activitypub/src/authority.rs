use std::fmt;

use mitra_models::users::types::User;
use mitra_utils::{
    crypto_eddsa::{
        ed25519_public_key_from_private_key,
        Ed25519PrivateKey,
        Ed25519PublicKey,
    },
    did_key::DidKey,
};

use super::did_url::DidApUrl;

pub(super) const GATEWAY_PATH_PREFIX: &str = "/.well-known/apgateway/";

fn fep_ef61_identity(public_key: &Ed25519PublicKey) -> DidApUrl {
    let did_key = DidKey::from_ed25519_key(public_key);
    DidApUrl::from_did_key(&did_key)
}

pub enum Authority {
    Server(String),
    // TODO: remove server URL after transition
    Key((String, Ed25519PublicKey)),
    KeyWithGateway((String, Ed25519PublicKey)),
}

impl fmt::Display for Authority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let authority_str = match self {
            Self::Server(ref server_url) => server_url.to_owned(),
            Self::Key((_, public_key)) => {
                fep_ef61_identity(public_key).to_string()
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
    fn key(server_url: &str, secret_key: &Ed25519PrivateKey) -> Self {
        let public_key = ed25519_public_key_from_private_key(secret_key);
        Self::Key((server_url.to_owned(), public_key))
    }

    fn key_with_gateway(server_url: &str, secret_key: &Ed25519PrivateKey) -> Self {
        let public_key = ed25519_public_key_from_private_key(secret_key);
        Self::KeyWithGateway((server_url.to_owned(), public_key))
    }

    pub fn from_user(
        server_url: &str,
        user: &User,
        fep_ef61_enabled: bool,
    ) -> Self {
        if fep_ef61_enabled {
            Self::key_with_gateway(server_url, &user.ed25519_private_key)
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

    pub fn as_did_url(&self) -> Option<DidApUrl> {
        match self {
            Self::Server(_) => None,
            Self::Key((_, public_key)) | Self::KeyWithGateway((_, public_key)) => {
                Some(fep_ef61_identity(public_key))
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use mitra_utils::{
        crypto_eddsa::generate_weak_ed25519_key,
        urls::Url,
    };
    use super::*;

    const SERVER_URL: &str = "https://server.example";

    #[test]
    fn test_authority_server() {
        let authority = Authority::server(SERVER_URL);
        assert!(!authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example");
        assert_eq!(authority.server_url(), SERVER_URL);
        assert_eq!(authority.as_did_url().is_none(), true);
    }

    #[test]
    fn test_authority_key() {
        let secret_key = generate_weak_ed25519_key();
        let authority = Authority::key(SERVER_URL, &secret_key);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.server_url(), SERVER_URL);
        assert_eq!(authority.as_did_url().is_some(), true);
    }

    #[test]
    fn test_authority_key_with_gateway() {
        let secret_key = generate_weak_ed25519_key();
        let authority = Authority::key_with_gateway(SERVER_URL, &secret_key);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example/.well-known/apgateway/did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.server_url(), SERVER_URL);
        assert_eq!(authority.as_did_url().is_some(), true);
    }

    #[test]
    fn test_fep_ef61_identity_url_compat() {
        let secret_key = generate_weak_ed25519_key();
        let public_key = ed25519_public_key_from_private_key(&secret_key);
        let did_ap_key = fep_ef61_identity(&public_key);
        let did_url = format!("{did_ap_key}/objects/1");
        assert_eq!(did_url, "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1");
        let url = Url::parse(&did_url).unwrap();
        assert_eq!(url.scheme(), "did");
        assert_eq!(url.authority(), "");
        assert_eq!(url.path(), "ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1");
        assert_eq!(url.to_string(), did_url);
    }
}
