use std::fmt;

use mitra_models::users::types::User;
use mitra_utils::{
    crypto_eddsa::{
        ed25519_public_key_from_private_key,
        Ed25519PrivateKey,
    },
    did_key::DidKey,
};

fn fep_ef61_identity(secret_key: &Ed25519PrivateKey) -> String {
    let public_key = ed25519_public_key_from_private_key(secret_key);
    let did_key = DidKey::from_ed25519_key(public_key.as_bytes());
    format!(
        "did:ap:key:{}",
        did_key.key_multibase(),
    )
}

pub enum Authority {
    Server(String),
    ServerKey((String, Ed25519PrivateKey)),
}

impl fmt::Display for Authority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let authority_str = match self {
            Self::Server(ref base_url) => base_url.to_owned(),
            Self::ServerKey((_, secret_key)) => {
                fep_ef61_identity(secret_key)
            },
        };
        write!(formatter, "{}", authority_str)
    }
}

impl Authority {
    pub fn server(base_url: &str) -> Self {
        Self::Server(base_url.to_owned())
    }

    pub fn server_key(base_url: &str, secret_key: &Ed25519PrivateKey) -> Self {
        Self::ServerKey((base_url.to_owned(), *secret_key))
    }

    pub fn from_user(
        base_url: &str,
        user: &User,
        fep_ef61_enabled: bool,
    ) -> Self {
        if fep_ef61_enabled {
            Self::server_key(base_url, &user.ed25519_private_key)
        } else {
            Self::server(base_url)
        }
    }

    pub fn is_fep_ef61(&self) -> bool {
        !matches!(self, Self::Server(_))
    }

    pub fn base_url(&self) -> &str {
        match self {
            Self::Server(ref base_url) => base_url,
            Self::ServerKey((ref base_url, _)) => base_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use mitra_utils::crypto_eddsa::generate_weak_ed25519_key;
    use super::*;

    const BASE_URL: &str = "https://server.example";

    #[test]
    fn test_authority() {
        let authority = Authority::server(BASE_URL);
        assert!(!authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "https://server.example");
        assert_eq!(authority.base_url(), BASE_URL);
    }

    #[test]
    fn test_authority_fep_ef61() {
        let secret_key = generate_weak_ed25519_key();
        let authority = Authority::server_key(BASE_URL, &secret_key);
        assert!(authority.is_fep_ef61());
        assert_eq!(authority.to_string(), "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(authority.base_url(), BASE_URL);
    }
}
