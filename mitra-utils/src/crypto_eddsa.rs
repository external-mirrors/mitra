use ed25519_dalek::{
    Keypair,
    PublicKey,
    SecretKey,
    Signature,
    SignatureError,
    Signer,
    Verifier,
};

pub type EddsaError = SignatureError;

pub fn generate_eddsa_keypair() -> Keypair {
    let mut rng = rand_0_7::rngs::OsRng;
    Keypair::generate(&mut rng)
}

#[cfg(feature = "test-utils")]
pub fn generate_weak_eddsa_keypair() -> Keypair {
    use rand_0_7::SeedableRng;
    let mut rng = rand_0_7::rngs::StdRng::seed_from_u64(0);
    Keypair::generate(&mut rng)
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
        let keypair = generate_eddsa_keypair();
        let message = "test";
        let signature = create_eddsa_signature(
            keypair.secret.to_bytes(),
            message.as_bytes(),
        ).unwrap();
        let result = verify_eddsa_signature(
            keypair.public.to_bytes(),
            message.as_bytes(),
            signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
