use crate::{
    crypto_eddsa::{
        ed25519_public_key_from_secret_key,
        Ed25519PublicKey,
        Ed25519SecretKey,
    },
    crypto_rsa::{RsaPublicKey, RsaSecretKey},
};

pub enum SecretKey {
    Ed25519(Ed25519SecretKey),
    Rsa(RsaSecretKey),
}

pub enum PublicKey {
    Ed25519(Ed25519PublicKey),
    Rsa(RsaPublicKey),
}

impl SecretKey {
    pub fn public_key(&self) -> PublicKey {
        match self {
            Self::Ed25519(secret_key) => {
                let public_key =
                    ed25519_public_key_from_secret_key(secret_key);
                PublicKey::Ed25519(public_key)
            },
            Self::Rsa(rsa_secret_key) => {
                let public_key = RsaPublicKey::from(rsa_secret_key);
                PublicKey::Rsa(public_key)
            },
        }
    }
}
