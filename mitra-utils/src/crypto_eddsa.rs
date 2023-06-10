use ed25519_dalek::{
    Keypair,
    PublicKey,
    SecretKey,
    Signature,
    SignatureError,
    Signer,
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
    private_key: [u8; 32],
    message: &[u8],
) -> Result<[u8; 64], SignatureError> {
    let secret_key = SecretKey::from_bytes(&private_key)?;
    let public_key = PublicKey::from(&secret_key);
    let keypair = Keypair { secret: secret_key, public: public_key };
    let signature = keypair.sign(message);
    Ok(signature.to_bytes())
}

pub fn verify_eddsa_signature(
    public_key: [u8; 32],
    message: &[u8],
    signature: [u8; 64],
) -> Result<(), SignatureError> {
    let public_key = PublicKey::from_bytes(&public_key)?;
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
            private_key.to_bytes(),
            message.as_bytes(),
        ).unwrap();
        let public_key = PublicKey::from(&private_key);
        let result = verify_eddsa_signature(
            public_key.to_bytes(),
            message.as_bytes(),
            signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
