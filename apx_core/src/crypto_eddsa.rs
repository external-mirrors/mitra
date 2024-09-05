// Using ed25519 v1.5
// because ed25519 v2.2 requires Rust 1.65 (via pkcs8 dependency)
use ed25519_1::pkcs8::{
    DecodePublicKey,
    PublicKeyBytes,
};
use ed25519_dalek::{
    SecretKey,
    SigningKey,
    Signature,
    SignatureError,
    Signer,
    Verifier,
    VerifyingKey,
};

use crate::{
    multibase::{decode_multibase_base58btc, encode_multibase_base58btc},
    multicodec::Multicodec,
};

pub type Ed25519SecretKey = SecretKey;
pub type Ed25519PublicKey = VerifyingKey;
pub type EddsaError = SignatureError;

pub fn generate_ed25519_key() -> SecretKey {
    let mut rng = rand::rngs::OsRng;
    let keypair = SigningKey::generate(&mut rng);
    keypair.to_bytes()
}

#[cfg(any(test, feature = "test-utils"))]
pub fn generate_weak_ed25519_key() -> SecretKey {
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0);
    let keypair = SigningKey::generate(&mut rng);
    keypair.to_bytes()
}

#[derive(thiserror::Error, Debug)]
pub enum Ed25519SerializationError {
    #[error("conversion error")]
    ConversionError,

    #[error(transparent)]
    KeyError(#[from] SignatureError),

    #[error("pkcs8 decoding error")]
    Pkcs8Error,

    #[error("multikey error")]
    MultikeyError,
}

pub fn ed25519_secret_key_from_bytes(
    bytes: &[u8],
) -> Result<SecretKey, Ed25519SerializationError> {
    let secret_key: SecretKey = bytes.try_into()
        .map_err(|_| Ed25519SerializationError::ConversionError)?;
    Ok(secret_key)
}

pub fn ed25519_secret_key_to_multikey(
    secret_key: &Ed25519SecretKey,
) -> String {
    let secret_key_multicode = Multicodec::Ed25519Priv.encode(secret_key);
    encode_multibase_base58btc(&secret_key_multicode)
}

pub fn ed25519_secret_key_from_multikey(
    secret_key_multibase: &str,
) -> Result<Ed25519SecretKey, Ed25519SerializationError> {
    let secret_key_multicode = decode_multibase_base58btc(secret_key_multibase)
        .map_err(|_| Ed25519SerializationError::MultikeyError)?;
    let secret_key_bytes =
        Multicodec::Ed25519Priv.decode_exact(&secret_key_multicode)
            .map_err(|_| Ed25519SerializationError::MultikeyError)?;
    let secret_key = ed25519_secret_key_from_bytes(&secret_key_bytes)?;
    Ok(secret_key)
}

pub fn ed25519_public_key_from_bytes(
    bytes: &[u8],
) -> Result<VerifyingKey, Ed25519SerializationError> {
    let bytes: [u8; 32] = bytes.try_into()
        .map_err(|_| Ed25519SerializationError::ConversionError)?;
    let public_key = VerifyingKey::from_bytes(&bytes)?;
    Ok(public_key)
}

pub fn ed25519_public_key_from_pkcs8_pem(
    public_key_pem: &str,
) -> Result<VerifyingKey, Ed25519SerializationError> {
    let public_key_bytes = PublicKeyBytes::from_public_key_pem(public_key_pem)
        .map_err(|_| Ed25519SerializationError::Pkcs8Error)?;
    let public_key = VerifyingKey::from_bytes(public_key_bytes.as_ref())?;
    Ok(public_key)
}

pub fn ed25519_public_key_from_secret_key(
    secret_key: &SecretKey,
) -> VerifyingKey {
    SigningKey::from(secret_key).verifying_key()
}

pub fn ed25519_public_key_to_multikey(
    public_key: &Ed25519PublicKey,
) -> String {
    let public_key_multicode =
        Multicodec::Ed25519Pub.encode(public_key.as_bytes());
    encode_multibase_base58btc(&public_key_multicode)
}

pub fn ed25519_public_key_from_multikey(
    multikey: &str,
) -> Result<Ed25519PublicKey, Ed25519SerializationError> {
    let public_key_multicode = decode_multibase_base58btc(multikey)
        .map_err(|_| Ed25519SerializationError::MultikeyError)?;
    let public_key_bytes =
        Multicodec::Ed25519Pub.decode_exact(&public_key_multicode)
            .map_err(|_| Ed25519SerializationError::MultikeyError)?;
    let public_key = ed25519_public_key_from_bytes(&public_key_bytes)?;
    Ok(public_key)
}

pub fn create_eddsa_signature(
    secret_key: &SecretKey,
    message: &[u8],
) -> [u8; 64] {
    let secret_key = SigningKey::from_bytes(secret_key);
    let signature = secret_key.sign(message);
    signature.to_bytes()
}

pub fn verify_eddsa_signature(
    public_key: &VerifyingKey,
    message: &[u8],
    signature: [u8; 64],
) -> Result<(), SignatureError> {
    let signature = Signature::from_bytes(&signature);
    public_key.verify(message, &signature)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_key_multikey_encode_decode() {
        let secret_key = generate_weak_ed25519_key();
        let encoded = ed25519_secret_key_to_multikey(&secret_key);
        let decoded = ed25519_secret_key_from_multikey(&encoded).unwrap();
        assert_eq!(decoded, secret_key);
    }

    #[test]
    fn test_public_key_multikey_encode_decode() {
        let secret_key = generate_weak_ed25519_key();
        let public_key = ed25519_public_key_from_secret_key(&secret_key);
        let encoded = ed25519_public_key_to_multikey(&public_key);
        let decoded = ed25519_public_key_from_multikey(&encoded).unwrap();
        assert_eq!(decoded, public_key);
    }

    #[test]
    fn test_verify_eddsa_signature() {
        let secret_key = generate_ed25519_key();
        let message = "test";
        let signature = create_eddsa_signature(
            &secret_key,
            message.as_bytes(),
        );
        let public_key =
            ed25519_public_key_from_secret_key(&secret_key);
        let result = verify_eddsa_signature(
            &public_key,
            message.as_bytes(),
            signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
