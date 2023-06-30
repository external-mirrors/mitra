use mitra_utils::did_pkh::DidPkh;

use super::signatures::{recover_address, SignatureError};
use super::utils::address_to_string;

#[derive(thiserror::Error, Debug)]
pub enum Eip191VerificationError {
    #[error(transparent)]
    InvalidSignature(#[from] SignatureError),

    #[error("invalid signer")]
    InvalidSigner,
}

pub fn verify_eip191_signature(
    did: &DidPkh,
    message: &str,
    signature_hex: &str,
) -> Result<(), Eip191VerificationError> {
    let signature_data = signature_hex.parse()?;
    let signer = recover_address(message.as_bytes(), &signature_data)?;
    if address_to_string(signer) != did.address().to_lowercase() {
        return Err(Eip191VerificationError::InvalidSigner);
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use web3::signing::{Key, SecretKeyRef};
    use mitra_utils::{
        currencies::Currency,
        eip191::{verify_eip191_signature as verify_eip191_signature_k256},
    };
    use crate::ethereum::{
        signatures::{
            generate_ecdsa_key,
            sign_message,
        },
        utils::address_to_string,
    };
    use super::*;

    const ETHEREUM: Currency = Currency::Ethereum;

    #[test]
    fn test_verify_eip191_signature() {
        let message = "test";
        let secret_key = generate_ecdsa_key();
        let secret_key_ref = SecretKeyRef::new(&secret_key);
        let secret_key_str = secret_key.display_secret().to_string();
        let address = address_to_string(secret_key_ref.address());
        let did = DidPkh::from_address(&ETHEREUM, &address);
        let signature = sign_message(&secret_key_str, message.as_bytes())
            .unwrap();
        let result = verify_eip191_signature(
            &did,
            message,
            &signature.to_string(),
        );
        assert_eq!(result.is_ok(), true);

        // Compare with k256 implementation
        let result_k256 = verify_eip191_signature_k256(
            &did,
            message,
            &signature.to_bytes(),
        );
        assert_eq!(result_k256.is_ok(), true);
    }
}
