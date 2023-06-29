/// https://eips.ethereum.org/EIPS/eip-191
use k256::{
    ecdsa::VerifyingKey,
    elliptic_curve::sec1::ToEncodedPoint,
};
use sha3::{Digest, Keccak256};

use super::crypto_ecdsa::{recover_ecdsa_public_key, EcdsaError};
use super::did_pkh::DidPkh;

fn prepare_eip191_message(message: &[u8]) -> Vec<u8> {
    [
        "\x19Ethereum Signed Message:\n".as_bytes(),
        message.len().to_string().as_bytes(),
        message,
    ].concat()
}

fn ecdsa_public_key_to_address(public_key: &VerifyingKey) -> [u8; 20] {
    let encoded_point = public_key.to_encoded_point(false);
    let public_key_hash = Keccak256::digest(&encoded_point.as_bytes()[1..]);
    let address = public_key_hash[12..].try_into()
        .expect("address size should be 20 bytes");
    address
}

fn address_to_string(address: [u8; 20]) -> String {
    format!("0x{}", hex::encode(address))
}

pub fn recover_address_eip191(
    message: &[u8],
    signature: [u8; 65],
) -> Result<[u8; 20], EcdsaError> {
    let eip191_message = prepare_eip191_message(message);
    let public_key = recover_ecdsa_public_key(
        &eip191_message,
        signature,
    )?;
    let address = ecdsa_public_key_to_address(&public_key);
    Ok(address)
}

#[derive(thiserror::Error, Debug)]
pub enum Eip191VerificationError {
    #[error("invalid signature length")]
    InvalidSignatureLength,

    #[error(transparent)]
    InvalidSignature(#[from] EcdsaError),

    #[error("invalid signer")]
    InvalidSigner,
}

pub fn verify_eip191_signature(
    signer: &DidPkh,
    message: &str,
    signature: &[u8],
) -> Result<(), Eip191VerificationError> {
    let signature: [u8; 65] = signature.try_into()
        .map_err(|_| Eip191VerificationError::InvalidSignatureLength)?;
    let recovered_bytes = recover_address_eip191(
        message.as_bytes(),
        signature,
    )?;
    let recovered = address_to_string(recovered_bytes);
    if recovered != signer.address().to_lowercase() {
        return Err(Eip191VerificationError::InvalidSigner);
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::crypto_ecdsa::{generate_ecdsa_key, create_ecdsa_signature};
    use crate::currencies::Currency;
    use super::*;

    #[test]
    fn test_verify_eip191_signature() {
        let private_key = generate_ecdsa_key();
        let public_key = private_key.verifying_key();
        let address_bytes = ecdsa_public_key_to_address(&public_key);
        let address = address_to_string(address_bytes);
        let signer = DidPkh::from_address(&Currency::Ethereum, &address);
        let message = "test";
        let eip191_message = prepare_eip191_message(message.as_bytes());
        let signature =
            create_ecdsa_signature(&private_key, &eip191_message).unwrap();
        let result = verify_eip191_signature(
            &signer,
            message,
            &signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
