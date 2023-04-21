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

pub fn ed25519_private_key_from_bytes(
    bytes: &[u8],
) -> Result<SecretKey, SignatureError> {
    let private_key = SecretKey::from_bytes(bytes)?;
    Ok(private_key)
}

pub fn ed25519_public_key_from_bytes(
    bytes: &[u8],
) -> Result<PublicKey, SignatureError> {
    let public_key = PublicKey::from_bytes(bytes)?;
    Ok(public_key)
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
        let public_key = PublicKey::from(&private_key);
        let result = verify_eddsa_signature(
            &public_key,
            message.as_bytes(),
            signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
