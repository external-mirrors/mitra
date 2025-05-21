//! # Public keys and secret keys

use crate::{
    crypto_eddsa::{
        ed25519_public_key_from_bytes,
        ed25519_public_key_from_pkcs8_pem,
        ed25519_public_key_from_secret_key,
        Ed25519PublicKey,
        Ed25519SecretKey,
    },
    crypto_rsa::{
        deserialize_rsa_public_key,
        rsa_public_key_from_pkcs1_der,
        RsaPublicKey,
        RsaSecretKey,
    },
    multibase::decode_multibase_base58btc,
    multicodec::Multicodec,
};

/// Public key
pub enum PublicKey {
    Ed25519(Ed25519PublicKey),
    Rsa(RsaPublicKey),
}

impl PublicKey {
    /// Parses multikey string
    pub fn from_multikey(public_key_multibase: &str) -> Result<Self, &'static str> {
        let public_key_multicode = decode_multibase_base58btc(public_key_multibase)
            .map_err(|_| "invalid key encoding")?;
        let public_key_decoded = Multicodec::decode(&public_key_multicode)
            .map_err(|_| "unexpected key type")?;
        let public_key = match public_key_decoded {
            (Multicodec::RsaPub, public_key_der) => {
                let public_key = rsa_public_key_from_pkcs1_der(&public_key_der)
                    .map_err(|_| "invalid key encoding")?;
                PublicKey::Rsa(public_key)
            },
            (Multicodec::Ed25519Pub, public_key_bytes) => {
                // Validate Ed25519 key
                let public_key = ed25519_public_key_from_bytes(&public_key_bytes)
                    .map_err(|_| "invalid key encoding")?;
                PublicKey::Ed25519(public_key)
            },
            _ => return Err("unexpected key type"),
        };
        Ok(public_key)
    }

    /// Parses public key in PEM format
    pub fn from_pem(public_key_pem: &str) -> Result<Self, &'static str> {
        let public_key = match deserialize_rsa_public_key(public_key_pem) {
            Ok(public_key) => PublicKey::Rsa(public_key),
            Err(_) => {
                let public_key = ed25519_public_key_from_pkcs8_pem(public_key_pem)
                    .map_err(|_| "unexpected key type")?;
                PublicKey::Ed25519(public_key)
            },
        };
        Ok(public_key)
    }
}

/// Secret key
pub enum SecretKey {
    Ed25519(Ed25519SecretKey),
    Rsa(RsaSecretKey),
}

impl SecretKey {
    /// Returns the public key corresponding to this secret key
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
