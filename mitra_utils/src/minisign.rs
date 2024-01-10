/// https://jedisct1.github.io/minisign/
use blake2::{Blake2b512, Digest};

use crate::{
    base64,
    crypto_eddsa::{
        ed25519_public_key_from_bytes,
        verify_eddsa_signature,
        Ed25519PublicKey,
        Ed25519SerializationError,
        EddsaError,
    },
    did_key::DidKey,
    multicodec::MulticodecError,
};

const MINISIGN_SIGNATURE_CODE: [u8; 2] = *b"Ed";
const MINISIGN_SIGNATURE_PREHASHED_CODE: [u8; 2] = *b"ED";

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("invalid format")]
    InvalidFormat,

    #[error("invalid encoding")]
    InvalidEncoding(#[from] base64::DecodeError),

    #[error("invalid key length")]
    InvalidKeyLength,

    #[error("invalid signature length")]
    InvalidSignatureLength,

    #[error("invalid signature type")]
    InvalidSignatureType,
}

// Public key format:
// base64(<signature_algorithm> || <key_id> || <public_key>)
fn parse_minisign_public_key(
    key_b64: &str,
) -> Result<[u8; 32], ParseError> {
    let key_bin = base64::decode(key_b64)?;
    if key_bin.len() != 42 {
        return Err(ParseError::InvalidKeyLength);
    };

    let mut signature_algorithm = [0; 2];
    let mut _key_id = [0; 8];
    let mut key = [0; 32];
    signature_algorithm.copy_from_slice(&key_bin[0..2]);
    _key_id.copy_from_slice(&key_bin[2..10]);
    key.copy_from_slice(&key_bin[10..42]);

    if signature_algorithm.as_ref() != MINISIGN_SIGNATURE_CODE {
        return Err(ParseError::InvalidSignatureType);
    };
    Ok(key)
}

fn parse_minisign_public_key_file(
    key_file: &str,
) -> Result<[u8; 32], ParseError> {
    let key_b64 = key_file.lines()
        .find(|line| !line.starts_with("untrusted comment"))
        .ok_or(ParseError::InvalidFormat)?;
    parse_minisign_public_key(key_b64)
}

pub fn minisign_key_to_did(key_file: &str) -> Result<DidKey, ParseError> {
    let key = parse_minisign_public_key_file(key_file)?;
    let did_key = DidKey::from_ed25519_key(&key);
    Ok(did_key)
}

#[derive(Debug, PartialEq)]
pub struct MinisignSignature {
    pub value: [u8; 64],
    pub is_prehashed: bool,
}

// Signature format:
// base64(<signature_algorithm> || <key_id> || <signature>)
pub fn parse_minisign_signature(
    signature_b64: &str,
) -> Result<MinisignSignature, ParseError> {
    let signature_bin = base64::decode(signature_b64)?;
    if signature_bin.len() != 74 {
        return Err(ParseError::InvalidSignatureLength);
    };

    let mut signature_algorithm = [0; 2];
    let mut _key_id = [0; 8];
    let mut signature = [0; 64];
    signature_algorithm.copy_from_slice(&signature_bin[0..2]);
    _key_id.copy_from_slice(&signature_bin[2..10]);
    signature.copy_from_slice(&signature_bin[10..74]);

    let is_prehashed = match signature_algorithm {
        MINISIGN_SIGNATURE_CODE => false,
        MINISIGN_SIGNATURE_PREHASHED_CODE => true,
        _ => return Err(ParseError::InvalidSignatureType),
    };
    Ok(MinisignSignature { value: signature, is_prehashed })
}

pub fn parse_minisign_signature_file(
    signature_file: &str,
) -> Result<MinisignSignature, ParseError> {
    let signature_b64 = signature_file.lines()
        .find(|line| !line.starts_with("untrusted comment"))
        .ok_or(ParseError::InvalidFormat)?;
    parse_minisign_signature(signature_b64)
}

#[derive(thiserror::Error, Debug)]
pub enum VerificationError {
    #[error(transparent)]
    MulticodecError(#[from] MulticodecError),

    #[error(transparent)]
    ParseError(#[from] ParseError),

    #[error(transparent)]
    KeyError(#[from] Ed25519SerializationError),

    #[error(transparent)]
    SignatureError(#[from] EddsaError),
}

fn verify_eddsa_blake2_signature(
    message: &str,
    signer: &Ed25519PublicKey,
    signature: [u8; 64],
) -> Result<(), VerificationError> {
    let mut hasher = Blake2b512::new();
    hasher.update(message);
    let hash = hasher.finalize();
    verify_eddsa_signature(signer, &hash, signature)?;
    Ok(())
}

pub fn verify_minisign_signature(
    signer: &DidKey,
    message: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let ed25519_key_bytes = signer.try_ed25519_key()?;
    let ed25519_key = ed25519_public_key_from_bytes(&ed25519_key_bytes)?;
    let ed25519_signature = signature.try_into()
        .map_err(|_| ParseError::InvalidSignatureLength)?;
    // TODO: don't add newline
    let message = format!("{}\n", message);
    verify_eddsa_blake2_signature(
        &message,
        &ed25519_key,
        ed25519_signature,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minisign_public_key_file() {
        let key_b64 =
            "RWS/wRxk57oX+FE4a1zQEPgx3OemUuLKbDMLOd2q6/panRBLaftX3Kpl";
        let key_file = concat!(
            "untrusted comment: minisign public key F817BAE7641CC1BF\n",
            "RWS/wRxk57oX+FE4a1zQEPgx3OemUuLKbDMLOd2q6/panRBLaftX3Kpl\n",
        );
        let result_1 = parse_minisign_public_key_file(key_b64).unwrap();
        let result_2 = parse_minisign_public_key_file(key_file).unwrap();
        assert_eq!(result_1, result_2);
    }

    #[test]
    fn test_parse_minisign_signature_file() {
        let signature_b64 =
            "RUS/wRxk57oX+P9JzukdVNh3WYisLQIW4aiyOvl4plV384/ZmmNSlihXBb/mJoDsTW5HYYseRIVAiidr+1+OQCxVxPlDeAN9dAs=";
        let signature_file = concat!(
            "untrusted comment: signature from minisign secret key\n",
            "RUS/wRxk57oX+P9JzukdVNh3WYisLQIW4aiyOvl4plV384/ZmmNSlihXBb/mJoDsTW5HYYseRIVAiidr+1+OQCxVxPlDeAN9dAs=\n",
            "trusted comment: timestamp:1687113267	file:input	hashed\n",
            "lMlFzwgrnUd6O/e6fERRwTIBfX+v1Wn9p5ZEZeGPV/bh1/WLXbh+ZHjAbEWAlaUV5RR90RvWxb9G2bF9LjXbDw==\n",
        );
        let result_1 = parse_minisign_signature_file(signature_b64).unwrap();
        let result_2 = parse_minisign_signature_file(signature_file).unwrap();
        assert_eq!(result_1, result_2);
    }

    #[test]
    fn test_verify_minisign_signature() {
        let minisign_key =
            "RWS/wRxk57oX+FE4a1zQEPgx3OemUuLKbDMLOd2q6/panRBLaftX3Kpl";
        let message = "test";
        let minisign_signature =
            "RUS/wRxk57oX+P9JzukdVNh3WYisLQIW4aiyOvl4plV384/ZmmNSlihXBb/mJoDsTW5HYYseRIVAiidr+1+OQCxVxPlDeAN9dAs=";
        let signer_key = parse_minisign_public_key(minisign_key).unwrap();
        let signer = DidKey::from_ed25519_key(&signer_key);
        let signature = parse_minisign_signature(minisign_signature).unwrap();
        assert_eq!(signature.is_prehashed, true);
        let result = verify_minisign_signature(
            &signer,
            message,
            &signature.value,
        );
        assert_eq!(result.is_ok(), true);
    }
}
