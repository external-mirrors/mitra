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

pub type Ed25519PrivateKey = SecretKey;
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
}

pub fn ed25519_private_key_from_bytes(
    bytes: &[u8],
) -> Result<SecretKey, Ed25519SerializationError> {
    let private_key: SecretKey = bytes.try_into()
        .map_err(|_| Ed25519SerializationError::ConversionError)?;
    Ok(private_key)
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

pub fn ed25519_public_key_from_private_key(
    private_key: &SecretKey,
) -> VerifyingKey {
    SigningKey::from(private_key).verifying_key()
}

pub fn create_eddsa_signature(
    private_key: &SecretKey,
    message: &[u8],
) -> [u8; 64] {
    let private_key = SigningKey::from_bytes(private_key);
    let signature = private_key.sign(message);
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
    fn test_verify_eddsa_signature() {
        let private_key = generate_ed25519_key();
        let message = "test";
        let signature = create_eddsa_signature(
            &private_key,
            message.as_bytes(),
        );
        let public_key =
            ed25519_public_key_from_private_key(&private_key);
        let result = verify_eddsa_signature(
            &public_key,
            message.as_bytes(),
            signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
