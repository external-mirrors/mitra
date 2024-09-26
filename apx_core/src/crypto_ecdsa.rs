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

pub fn generate_ecdsa_key() -> SigningKey {
    let mut rng = rand::rngs::OsRng;
    let signing_key = SigningKey::random(&mut rng);
    signing_key
}

pub fn create_ecdsa_signature(
    secret_key: &SigningKey,
    message: &[u8],
) -> Result<[u8; 65], EcdsaError> {
    let signature: recoverable::Signature = secret_key.sign(message);
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
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    use super::*;

    #[test]
    fn test_recover_ecdsa_public_key() {
        let expected_public_key = [3, 94, 184, 242, 120, 141, 254, 94, 76, 117, 65, 156, 34, 59, 244, 145, 216, 110, 203, 242, 198, 243, 154, 174, 45, 219, 47, 55, 177, 13, 252, 217, 152];
        let message = b"test";
        let signature = [139, 186, 31, 53, 124, 3, 168, 132, 172, 104, 85, 44, 205, 250, 140, 21, 3, 200, 166, 97, 136, 86, 188, 89, 176, 37, 248, 87, 189, 101, 239, 163, 118, 246, 102, 207, 60, 62, 201, 247, 195, 102, 59, 3, 46, 14, 15, 240, 178, 16, 243, 81, 140, 94, 91, 85, 27, 76, 105, 211, 78, 62, 201, 249, 1];
        let recovered_key = recover_ecdsa_public_key(
            message,
            signature,
        ).unwrap();
        assert_eq!(
            recovered_key.to_encoded_point(true).to_bytes().as_ref(),
            expected_public_key,
        );
    }

    #[test]
    fn test_sign_and_recover() {
        let secret_key = generate_ecdsa_key();
        let public_key = secret_key.verifying_key();
        let message = b"test";
        let signature =
            create_ecdsa_signature(&secret_key, message).unwrap();
        let recovered_key = recover_ecdsa_public_key(
            message,
            signature,
        ).unwrap();
        assert_eq!(recovered_key, public_key);
    }
}
