use k256::{
    ecdsa::{
        recoverable,
        signature::{
            Signature as _Signature,
            Signer as _Signer,
        },
        Error,
        Signature,
        SigningKey,
        VerifyingKey,
    },
};

pub type EcdsaError = Error;

#[allow(dead_code)]
pub fn generate_ecdsa_key() -> SigningKey {
    let mut rng = rand::rngs::OsRng;
    let signing_key = SigningKey::random(&mut rng);
    signing_key
}

#[allow(dead_code)]
pub fn create_ecdsa_signature(
    private_key: &SigningKey,
    message: &[u8],
) -> Result<[u8; 65], EcdsaError> {
    let signature: recoverable::Signature = private_key.sign(message);
    let signature_bytes: [u8; 65] = signature.as_ref().try_into()
        .expect("signature size should be 65 bytes");
    Ok(signature_bytes)
}

pub fn recover_ecdsa_public_key(
    message: &[u8],
    signature: [u8; 65],
) -> Result<VerifyingKey, EcdsaError> {
    let signature_raw = Signature::from_bytes(&signature[..64])?;
    let recovery_id = recoverable::Id::new(&signature[64] % 27)?;
    let recoverable_signature = recoverable::Signature::new(
        &signature_raw,
        recovery_id,
    )?;
    // Requires keccak256 feature
    let public_key = recoverable_signature.recover_verifying_key(message)?;
    Ok(public_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recover_ecdsa_public_key() {
        let private_key = generate_ecdsa_key();
        let public_key = private_key.verifying_key();
        let message = b"test";
        let signature =
            create_ecdsa_signature(&private_key, message).unwrap();
        let recovered_key = recover_ecdsa_public_key(
            message,
            signature,
        ).unwrap();
        assert_eq!(recovered_key, public_key);
    }
}
