//! ECDSA utilities
use k256::{
    ecdsa::{
        Error,
        RecoveryId,
        Signature,
        SigningKey,
        VerifyingKey,
    },
};
use sha3::{Digest, Keccak256};

pub type EcdsaError = Error;

pub fn generate_ecdsa_key() -> SigningKey {
    let mut rng = rand::rngs::OsRng;
    let signing_key = SigningKey::random(&mut rng);
    signing_key
}

fn prehash(message: &[u8]) -> [u8; 32] {
    Keccak256::digest(message).into()
}

pub fn create_ecdsa_signature(
    secret_key: &SigningKey,
    message: &[u8],
) -> Result<[u8; 65], EcdsaError> {
    let message_hash = prehash(message);
    let (signature, recovery_id) =
        secret_key.sign_prehash_recoverable(&message_hash)?;
    let mut signature_bytes = [0u8; 65];
    signature_bytes[..64].copy_from_slice(signature.to_bytes().as_slice());
    signature_bytes[64] = recovery_id.to_byte();
    Ok(signature_bytes)
}

pub fn recover_ecdsa_public_key(
    message: &[u8],
    signature_bytes: [u8; 65],
) -> Result<VerifyingKey, EcdsaError> {
    let signature = Signature::try_from(&signature_bytes[..64])?;
    let recovery_id = RecoveryId::from_byte(&signature_bytes[64] % 27)
        .ok_or(EcdsaError::new())?;
    let message_hash = prehash(message);
    let public_key = VerifyingKey::recover_from_prehash(
        &message_hash,
        &signature,
        recovery_id
    )?;
    Ok(public_key)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(recovered_key, *public_key);
    }
}
