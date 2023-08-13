use std::str::FromStr;

use serde_json::{Value as JsonValue};
use url::Url;

use mitra_utils::{
    canonicalization::{
        canonicalize_object,
        CanonicalizationError,
    },
    crypto_eddsa::{verify_eddsa_signature, Ed25519PublicKey},
    crypto_rsa::{verify_rsa_sha256_signature, RsaPublicKey},
    did::Did,
    did_key::DidKey,
    did_pkh::DidPkh,
    eip191::verify_eip191_signature,
    minisign::verify_minisign_signature,
    multibase::{decode_multibase_base58btc, MultibaseError},
};

use super::create::{
    prepare_jcs_sha256_data,
    IntegrityProof,
    IntegrityProofConfig,
    PROOF_KEY,
    PURPOSE_ASSERTION_METHOD,
    PURPOSE_AUTHENTICATION,
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
    pub object: JsonValue,
    pub proof_config: IntegrityProofConfig,
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
    let proof = object_map.remove(PROOF_KEY)
        .ok_or(VerificationError::NoProof)?;
    let IntegrityProof {
        proof_config,
        proof_value,
    } = serde_json::from_value(proof)
        .map_err(|_| VerificationError::InvalidProof("invalid proof"))?;
    if proof_config.proof_purpose != PURPOSE_ASSERTION_METHOD &&
        proof_config.proof_purpose != PURPOSE_AUTHENTICATION
    {
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
    let signature = decode_multibase_base58btc(&proof_value)?;
    let signature_data = JsonSignatureData {
        proof_type,
        signer,
        object,
        proof_config,
        signature,
    };
    Ok(signature_data)
}

pub fn verify_rsa_json_signature(
    signer_key: &RsaPublicKey,
    object: &JsonValue,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let canonical_object = canonicalize_object(object)?;
    let is_valid_signature = verify_rsa_sha256_signature(
        signer_key,
        &canonical_object,
        signature,
    );
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(())
}

pub fn verify_eddsa_json_signature(
    signer_key: &Ed25519PublicKey,
    object: &JsonValue,
    proof_config: &IntegrityProofConfig,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let hash_data = prepare_jcs_sha256_data(object, proof_config)?;
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
    object: &JsonValue,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let canonical_object = canonicalize_object(object)?;
    verify_eip191_signature(signer, &canonical_object, signature)
        .map_err(|_| VerificationError::InvalidSignature)
}

pub fn verify_blake2_ed25519_json_signature(
    signer: &DidKey,
    object: &JsonValue,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let canonical_object = canonicalize_object(object)?;
    verify_minisign_signature(signer, &canonical_object, signature)
        .map_err(|_| VerificationError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use mitra_utils::{
        crypto_eddsa::{
            generate_ed25519_key,
            ed25519_private_key_from_bytes,
            ed25519_public_key_from_bytes,
            Ed25519PublicKey,
        },
        crypto_rsa::generate_weak_rsa_key,
        currencies::Currency,
        multibase::decode_multibase_base58btc,
        multicodec::{
            decode_ed25519_private_key,
            decode_ed25519_public_key,
        },
    };
    use crate::create::{
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
        assert_eq!(signature_data.signature, [171, 205]);
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
            &signature_data.object,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_eddsa_signature() {
        let signer_key = generate_ed25519_key();
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
            &signer_key,
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

        let signer_public_key = Ed25519PublicKey::from(&signer_key);
        let result = verify_eddsa_json_signature(
            &signer_public_key,
            &signature_data.object,
            &signature_data.proof_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_eddsa_signature_test_vector() {
        let private_key_multibase = "z3u2en7t5LR2WtQH5PfFqMqwVHBeXouLzo6haApm8XHqvjxq";
        let private_key_multicode = decode_multibase_base58btc(private_key_multibase).unwrap();
        let private_key_bytes = decode_ed25519_private_key(&private_key_multicode).unwrap();
        let private_key = ed25519_private_key_from_bytes(&private_key_bytes).unwrap();
        let key_id = "https://server.example/users/alice#ed25519-key";
        let created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let object = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/data-integrity/v1"
            ],
            "type": "Create",
            "actor": "https://server.example/users/alice",
            "object": {
                "type": "Note",
                "content": "Hello world"
            }
        });
        let signed_object = sign_object_eddsa(
            &private_key,
            key_id,
            &object,
            Some(created_at),
        ).unwrap();

        let expected_result = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/data-integrity/v1"
            ],
            "type": "Create",
            "actor": "https://server.example/users/alice",
            "object": {
                "type": "Note",
                "content": "Hello world"
            },
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-jcs-2022",
                "verificationMethod": "https://server.example/users/alice#ed25519-key",
                "proofPurpose": "assertionMethod",
                "proofValue": "z3sXaxjKs4M3BRicwWA9peyNPJvJqxtGsDmpt1jjoHCjgeUf71TRFz56osPSfDErszyLp5Ks1EhYSgpDaNM977Rg2",
                "created": "2023-02-24T23:36:38Z"
            }
        });
        assert_eq!(signed_object, expected_result);

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::JcsEddsaSignature,
        );
        let public_key_multibase = "z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let public_key_multicode = decode_multibase_base58btc(public_key_multibase).unwrap();
        let public_key_bytes = decode_ed25519_public_key(&public_key_multicode).unwrap();
        let public_key = ed25519_public_key_from_bytes(&public_key_bytes).unwrap();
        let result = verify_eddsa_json_signature(
            &public_key,
            &signature_data.object,
            &signature_data.proof_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
