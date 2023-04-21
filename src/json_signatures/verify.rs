use std::str::FromStr;

use serde_json::{Value as JsonValue};
use sha2::{Digest, Sha256};
use url::Url;

use mitra_utils::{
    canonicalization::{
        canonicalize_object,
        CanonicalizationError,
    },
    crypto_eddsa::verify_eddsa_signature,
    crypto_rsa::{verify_rsa_sha256_signature, RsaPublicKey},
    did::Did,
    did_key::DidKey,
    did_pkh::DidPkh,
    multibase::{decode_multibase_base58btc, MultibaseError},
};

use crate::ethereum::identity::verify_eip191_signature;
use crate::identity::{
    minisign::verify_minisign_signature,
};
use super::create::{
    IntegrityProof,
    PROOF_KEY,
    PROOF_PURPOSE,
};
use super::proofs::{ProofType, DATA_INTEGRITY_PROOF};

#[derive(Debug, PartialEq)]
pub enum JsonSigner {
    ActorKeyId(String),
    Did(Did),
}

pub struct JsonSignatureData {
    pub proof_type: ProofType,
    pub signer: JsonSigner,
    pub canonical_object: String,
    pub canonical_config: String,
    pub signature: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum JsonSignatureVerificationError {
    #[error("invalid object")]
    InvalidObject,

    #[error("no proof")]
    NoProof,

    #[error("{0}")]
    InvalidProof(&'static str),

    #[error(transparent)]
    CanonicalizationError(#[from] CanonicalizationError),

    #[error("invalid encoding")]
    InvalidEncoding(#[from] MultibaseError),

    #[error("invalid signature")]
    InvalidSignature,
}

type VerificationError = JsonSignatureVerificationError;

pub fn get_json_signature(
    object: &JsonValue,
) -> Result<JsonSignatureData, VerificationError> {
    let mut object = object.clone();
    let object_map = object.as_object_mut()
        .ok_or(VerificationError::InvalidObject)?;
    let proof_value = object_map.remove(PROOF_KEY)
        .ok_or(VerificationError::NoProof)?;
    let IntegrityProof {
        proof_config,
        proof_value,
    } = serde_json::from_value(proof_value)
        .map_err(|_| VerificationError::InvalidProof("invalid proof"))?;
    if proof_config.proof_purpose != PROOF_PURPOSE {
        return Err(VerificationError::InvalidProof("invalid proof purpose"));
    };
    let proof_type = if proof_config.proof_type == DATA_INTEGRITY_PROOF {
        let cryptosuite = proof_config.cryptosuite.as_ref()
            .ok_or(VerificationError::InvalidProof("cryptosuite is not specified"))?;
        ProofType::from_cryptosuite(cryptosuite)
            .map_err(|_| VerificationError::InvalidProof("unsupported proof type"))?
    } else {
        proof_config.proof_type.parse()
            .map_err(|_| VerificationError::InvalidProof("unsupported proof type"))?
    };
    let signer = if let Ok(did) = Did::from_str(&proof_config.verification_method) {
        JsonSigner::Did(did)
    } else if Url::parse(&proof_config.verification_method).is_ok() {
        JsonSigner::ActorKeyId(proof_config.verification_method.clone())
    } else {
        return Err(VerificationError::InvalidProof("unsupported verification method"));
    };
    let canonical_object = canonicalize_object(&object)?;
    let canonical_config = canonicalize_object(&proof_config)?;
    let signature = decode_multibase_base58btc(&proof_value)?;
    let signature_data = JsonSignatureData {
        proof_type,
        signer,
        canonical_object,
        canonical_config,
        signature,
    };
    Ok(signature_data)
}

pub fn verify_rsa_json_signature(
    signer_key: &RsaPublicKey,
    canonical_object: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let is_valid_signature = verify_rsa_sha256_signature(
        signer_key,
        canonical_object,
        signature,
    );
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(())
}

#[allow(dead_code)]
pub fn verify_eddsa_json_signature(
    signer_key: [u8; 32],
    canonical_object: &str,
    canonical_config: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let object_hash = Sha256::digest(canonical_object.as_bytes());
    let config_hash = Sha256::digest(canonical_config.as_bytes());
    let hash_data = [config_hash, object_hash].concat();
    let signature: [u8; 64] = signature.try_into()
        .map_err(|_| VerificationError::InvalidSignature)?;
    verify_eddsa_signature(
        signer_key,
        &hash_data,
        signature,
    ).map_err(|_| VerificationError::InvalidSignature)?;
    Ok(())
}

pub fn verify_eip191_json_signature(
    signer: &DidPkh,
    canonical_object: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let signature_hex = hex::encode(signature);
    verify_eip191_signature(signer, canonical_object, &signature_hex)
        .map_err(|_| VerificationError::InvalidSignature)
}

pub fn verify_blake2_ed25519_json_signature(
    signer: &DidKey,
    canonical_object: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    verify_minisign_signature(signer, canonical_object, signature)
        .map_err(|_| VerificationError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use mitra_utils::{
        crypto_eddsa::generate_eddsa_keypair,
        crypto_rsa::generate_weak_rsa_key,
        currencies::Currency,
    };
    use crate::json_signatures::create::{
        sign_object_eddsa,
        sign_object_rsa,
    };
    use super::*;

    #[test]
    fn test_get_json_signature_eip191() {
        let signed_object = json!({
            "type": "Test",
            "id": "https://example.org/objects/1",
            "proof": {
                "type": "JcsEip191Signature2022",
                "proofPurpose": "assertionMethod",
                "verificationMethod": "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a",
                "created": "2020-11-05T19:23:24Z",
                "proofValue": "zE5J",
            },
        });
        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::JcsEip191Signature,
        );
        let expected_signer = JsonSigner::Did(Did::Pkh(DidPkh::from_address(
            &Currency::Ethereum,
            "0xb9c5714089478a327f09197987f16f9e5d936e8a",
        )));
        assert_eq!(signature_data.signer, expected_signer);
        assert_eq!(hex::encode(signature_data.signature), "abcd");
    }

    #[test]
    fn test_create_and_verify_rsa_signature() {
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://example.org/users/test#main-key";
        let object = json!({
            "type": "Create",
            "actor": "https://example.org/users/test",
            "id": "https://example.org/objects/1",
            "to": [
                "https://example.org/users/yyy",
                "https://example.org/users/xxx",
            ],
            "object": {
                "type": "Note",
                "content": "test",
            },
        });
        let signed_object = sign_object_rsa(
            &signer_key,
            signer_key_id,
            &object,
            None,
        ).unwrap();

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::JcsRsaSignature,
        );
        let expected_signer = JsonSigner::ActorKeyId(signer_key_id.to_string());
        assert_eq!(signature_data.signer, expected_signer);

        let signer_public_key = RsaPublicKey::from(signer_key);
        let result = verify_rsa_json_signature(
            &signer_public_key,
            &signature_data.canonical_object,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_eddsa_signature() {
        let signer_keypair = generate_eddsa_keypair();
        let signer_key_id = "https://example.org/users/test#main-key";
        let object = json!({
            "type": "Create",
            "actor": "https://example.org/users/test",
            "id": "https://example.org/objects/1",
            "to": [
                "https://example.org/users/yyy",
                "https://example.org/users/xxx",
            ],
            "object": {
                "type": "Note",
                "content": "test",
            },
        });
        let signed_object = sign_object_eddsa(
            signer_keypair.secret.to_bytes(),
            signer_key_id,
            &object,
            None,
        ).unwrap();

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::JcsEddsaSignature,
        );
        let expected_signer = JsonSigner::ActorKeyId(signer_key_id.to_string());
        assert_eq!(signature_data.signer, expected_signer);

        let signer_public_key = signer_keypair.public.to_bytes();
        let result = verify_eddsa_json_signature(
            signer_public_key,
            &signature_data.canonical_object,
            &signature_data.canonical_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
