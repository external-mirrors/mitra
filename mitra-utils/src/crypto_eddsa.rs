use ed25519::pkcs8::{
    DecodePublicKey,
    PublicKeyBytes,
};
use ed25519_dalek::{
    ExpandedSecretKey,
    Keypair,
    PublicKey,
    SecretKey,
    Signature,
    SignatureError,
    Verifier,
};

pub type Ed25519PrivateKey = SecretKey;
pub type Ed25519PublicKey = PublicKey;
pub type EddsaError = SignatureError;

pub fn generate_ed25519_key() -> SecretKey {
    let mut rng = rand_0_7::rngs::OsRng;
    let keypair = Keypair::generate(&mut rng);
    keypair.secret
}

#[cfg(feature = "test-utils")]
pub fn generate_weak_ed25519_key() -> SecretKey {
    use rand_0_7::SeedableRng;
    let mut rng = rand_0_7::rngs::StdRng::seed_from_u64(0);
    let keypair = Keypair::generate(&mut rng);
    keypair.secret
}

#[derive(thiserror::Error, Debug)]
pub enum Ed25519SerializationError {
    #[error(transparent)]
    KeyError(#[from] SignatureError),

    #[error("pkcs8 decoding error")]
    Pkcs8Error,
}

pub fn ed25519_private_key_from_bytes(
    bytes: &[u8],
) -> Result<SecretKey, Ed25519SerializationError> {
    let private_key = SecretKey::from_bytes(bytes)?;
    Ok(private_key)
}

pub fn ed25519_public_key_from_bytes(
    bytes: &[u8],
) -> Result<PublicKey, Ed25519SerializationError> {
    let public_key = PublicKey::from_bytes(bytes)?;
    Ok(public_key)
}

pub fn ed25519_public_key_from_pkcs8_pem(
    public_key_pem: &str,
) -> Result<PublicKey, Ed25519SerializationError> {
    let public_key_bytes = PublicKeyBytes::from_public_key_pem(public_key_pem)
        .map_err(|_| Ed25519SerializationError::Pkcs8Error)?;
    let public_key = PublicKey::from_bytes(public_key_bytes.as_ref())?;
    Ok(public_key)
}

pub fn ed25519_public_key_from_private_key(
    private_key: &SecretKey,
) -> PublicKey {
    PublicKey::from(private_key)
}

pub fn create_eddsa_signature(
    private_key: &SecretKey,
    message: &[u8],
) -> [u8; 64] {
    let public_key = PublicKey::from(private_key);
    let expanded_private_key = ExpandedSecretKey::from(private_key);
    let signature = expanded_private_key.sign(message, &public_key);
    signature.to_bytes()
}

pub fn verify_eddsa_signature(
    public_key: &PublicKey,
    message: &[u8],
    signature: [u8; 64],
) -> Result<(), SignatureError> {
    let signature = Signature::from_bytes(&signature)?;
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
