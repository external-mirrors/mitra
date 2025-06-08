//! Create JSON signatures
//!
//! <https://www.w3.org/TR/vc-data-integrity/>
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::{
    crypto_eddsa::{
        create_eddsa_signature,
        Ed25519SecretKey,
        EddsaError,
    },
    crypto_rsa::{
        create_rsa_sha256_signature,
        RsaError,
        RsaSecretKey,
    },
    jcs::{
        canonicalize_object,
        CanonicalizationError,
    },
    multibase::encode_multibase_base58btc,
};

use super::proofs::{
    CRYPTOSUITE_JCS_EDDSA,
    CRYPTOSUITE_JCS_EDDSA_LEGACY,
    DATA_INTEGRITY_PROOF,

    PROOF_TYPE_JCS_RSA,
};

#[cfg(feature = "eip191")]
use crate::did_pkh::DidPkh;
#[cfg(feature = "eip191")]
use super::proofs::PROOF_TYPE_JCS_EIP191;

#[cfg(feature = "minisign")]
use crate::did_key::DidKey;
#[cfg(feature = "minisign")]
use super::proofs::PROOF_TYPE_JCS_BLAKE2_ED25519;

pub(super) const PROOF_KEY: &str = "proof";
pub(super) const LD_SIGNATURE_KEY: &str = "signature";
pub(super) const PURPOSE_ASSERTION_METHOD: &str = "assertionMethod";
pub(super) const PURPOSE_AUTHENTICATION: &str = "authentication";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityProofConfig {
    #[serde(rename = "type")]
    pub proof_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cryptosuite: Option<String>,
    pub proof_purpose: String,
    pub verification_method: String,
    pub created: DateTime<Utc>,
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    _context: Option<JsonValue>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityProof {
    #[serde(flatten)]
    proof_config: IntegrityProofConfig,
    proof_value: String,
}

impl IntegrityProofConfig {
    pub fn jcs_eddsa(
        verification_method: &str,
        created_at: DateTime<Utc>,
        maybe_proof_context: Option<JsonValue>,
    ) -> Self {
        Self {
            proof_type: DATA_INTEGRITY_PROOF.to_string(),
            cryptosuite: Some(CRYPTOSUITE_JCS_EDDSA.to_string()),
            proof_purpose: PURPOSE_ASSERTION_METHOD.to_string(),
            verification_method: verification_method.to_string(),
            created: created_at,
            _context: maybe_proof_context,
        }
    }

    pub fn jcs_eddsa_legacy(
        verification_method: &str,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            proof_type: DATA_INTEGRITY_PROOF.to_string(),
            cryptosuite: Some(CRYPTOSUITE_JCS_EDDSA_LEGACY.to_string()),
            proof_purpose: PURPOSE_ASSERTION_METHOD.to_string(),
            verification_method: verification_method.to_string(),
            created: created_at,
            _context: None,
        }
    }
}

impl IntegrityProof {
    pub fn new(
        proof_config: IntegrityProofConfig,
        signature: &[u8],
    ) -> Self {
        Self {
            proof_config,
            proof_value: encode_multibase_base58btc(signature),
        }
    }

    fn jcs_rsa(
        signer_key_id: &str,
        signature: &[u8],
        created_at: DateTime<Utc>,
    ) -> Self {
        let proof_config = IntegrityProofConfig {
            proof_type: PROOF_TYPE_JCS_RSA.to_string(),
            cryptosuite: None,
            proof_purpose: PURPOSE_ASSERTION_METHOD.to_string(),
            verification_method: signer_key_id.to_string(),
            created: created_at,
            _context: None,
        };
        Self::new(proof_config, signature)
    }

    #[cfg(feature = "eip191")]
    pub fn jcs_eip191(
        signer: &DidPkh,
        signature: &[u8],
    ) -> Self {
        let proof_config = IntegrityProofConfig {
            proof_type: PROOF_TYPE_JCS_EIP191.to_string(),
            cryptosuite: None,
            proof_purpose: PURPOSE_ASSERTION_METHOD.to_string(),
            verification_method: signer.to_string(),
            created: Utc::now(),
            _context: None,
        };
        Self::new(proof_config, signature)
    }

    #[cfg(feature = "minisign")]
    pub fn jcs_blake2_ed25519(
        signer: &DidKey,
        signature: &[u8],
    ) -> Self {
        let proof_config = IntegrityProofConfig {
            proof_type: PROOF_TYPE_JCS_BLAKE2_ED25519.to_string(),
            cryptosuite: None,
            proof_purpose: PURPOSE_ASSERTION_METHOD.to_string(),
            verification_method: signer.to_string(),
            created: Utc::now(),
            _context: None,
        };
        Self::new(proof_config, signature)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum JsonSignatureError {
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),

    #[error("@context is required")]
    ContextRequired,

    #[error(transparent)]
    CanonicalizationError(#[from] CanonicalizationError),

    #[error("signing error")]
    RsaError(#[from] RsaError),

    #[error("signing error")]
    EddsaError(#[from] EddsaError),

    #[error("invalid object")]
    InvalidObject,

    #[error("already signed")]
    AlreadySigned,
}

fn add_integrity_proof(
    object_value: &mut JsonValue,
    proof: IntegrityProof,
) -> Result<(), JsonSignatureError> {
    let object_map = object_value.as_object_mut()
        .ok_or(JsonSignatureError::InvalidObject)?;
    if object_map.contains_key(PROOF_KEY) {
        return Err(JsonSignatureError::AlreadySigned);
    };
    let proof_value = serde_json::to_value(proof)?;
    object_map.insert(PROOF_KEY.to_string(), proof_value);
    Ok(())
}

#[deprecated]
pub fn sign_object_rsa(
    signer_key: &RsaSecretKey,
    signer_key_id: &str,
    object: &JsonValue,
    current_time: Option<DateTime<Utc>>,
) -> Result<JsonValue, JsonSignatureError> {
    // Canonicalize
    let canonical_object = canonicalize_object(object)?;
    // Sign
    let signature = create_rsa_sha256_signature(
        signer_key,
        canonical_object.as_bytes(),
    )?;
    let signature_created_at = current_time.unwrap_or(Utc::now());
    // Insert proof
    let proof = IntegrityProof::jcs_rsa(
        signer_key_id,
        &signature,
        signature_created_at,
    );
    let mut signed_object = object.clone();
    add_integrity_proof(&mut signed_object, proof)?;
    Ok(signed_object)
}

pub fn prepare_jcs_sha256_data(
    object: &impl Serialize,
    proof_config: &impl Serialize,
) -> Result<Vec<u8>, CanonicalizationError> {
    let canonical_object = canonicalize_object(object)?;
    let object_hash = Sha256::digest(canonical_object.as_bytes());
    let canonical_proof_config = canonicalize_object(&proof_config)?;
    let proof_config_hash = Sha256::digest(canonical_proof_config.as_bytes());
    let hash_data = [proof_config_hash, object_hash].concat();
    Ok(hash_data)
}

pub fn sign_object_eddsa(
    signer_key: &Ed25519SecretKey,
    signer_key_id: &str,
    object: &JsonValue,
    current_time: Option<DateTime<Utc>>,
    use_legacy_cryptosuite: bool,
    use_proof_context: bool,
    require_context: bool,
) -> Result<JsonValue, JsonSignatureError> {
    let signature_created_at = current_time.unwrap_or(Utc::now());
    let proof_config = if use_legacy_cryptosuite {
        // jcs-eddsa-2022
        IntegrityProofConfig::jcs_eddsa_legacy(
            signer_key_id,
            signature_created_at,
        )
    } else {
        // eddsa-jcs-2022
        let maybe_context = if use_proof_context {
            // @context SHOULD be injected
            // https://www.w3.org/TR/vc-data-integrity/#context-injection
            let maybe_context = object.get("@context");
            if require_context && maybe_context.is_none() {
                return Err(JsonSignatureError::ContextRequired);
            };
            maybe_context
        } else {
            None
        };
        IntegrityProofConfig::jcs_eddsa(
            signer_key_id,
            signature_created_at,
            maybe_context.cloned(),
        )
    };
    let hash_data = prepare_jcs_sha256_data(object, &proof_config)?;
    let signature = create_eddsa_signature(signer_key, &hash_data);
    let proof = IntegrityProof::new(proof_config, &signature);
    let mut signed_object = object.clone();
    add_integrity_proof(&mut signed_object, proof)?;
    Ok(signed_object)
}

/// Adds integrity proof to a JSON document  
/// <https://codeberg.org/silverpill/feps/src/branch/main/8b32/fep-8b32.md>
pub fn sign_object(
    signer_key: &Ed25519SecretKey,
    signer_key_id: &str,
    object: &JsonValue,
) -> Result<JsonValue, JsonSignatureError> {
    sign_object_eddsa(
        signer_key,
        signer_key_id,
        object,
        None,
        false, // use eddsa-jcs-2022
        false, // no proof context
        false, // context injection not required
    )
}

pub fn is_object_signed(object: &JsonValue) -> bool {
    object.get(PROOF_KEY).is_some()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::{
        crypto_eddsa::generate_weak_ed25519_key,
        crypto_rsa::generate_weak_rsa_key,
    };
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn test_sign_object_rsa() {
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
        let current_time = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let result = sign_object_rsa(
            &signer_key,
            signer_key_id,
            &object,
            Some(current_time),
        ).unwrap();

        assert!(is_object_signed(&result));

        let expected_result = json!({
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
            "proof": {
                "type": "MitraJcsRsaSignature2022",
                "created": "2023-02-24T23:36:38Z",
                "verificationMethod": "https://example.org/users/test#main-key",
                "proofPurpose": "assertionMethod",
                "proofValue": "z4vYn27QHCnW8Lj3o6R9GCRp85BuM3SP2JoMCysBMhvEKu3mnR3FNEDWNtPaJCo27mWqmB68FxR2bppnAr4Qrvxu5",
            },
        });
        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_sign_object_eddsa() {
        let signer_key = generate_weak_ed25519_key();
        let signer_key_id = "https://example.org/users/test#main-key";
        let object = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
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
        let created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let result = sign_object_eddsa(
            &signer_key,
            signer_key_id,
            &object,
            Some(created_at),
            false,
            false, // no proof context
            false, // context injection not required
        ).unwrap();

        let expected_result = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
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
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-jcs-2022",
                "created": "2023-02-24T23:36:38Z",
                "verificationMethod": "https://example.org/users/test#main-key",
                "proofPurpose": "assertionMethod",
                "proofValue": "z4vVxpyT57PqoymsDZR9JmNvmkwhgFDY1gh2D54ayzQ5dnBJukvnXKdVcMiTgdsJn4PrivZguS7Ln68QvDknyyACZ",
            },
        });
        assert_eq!(result, expected_result);
    }

    #[test]
    fn test_sign_object_eddsa_context_required() {
        let signer_key = generate_weak_ed25519_key();
        let signer_key_id = "https://example.org/users/test#main-key";
        let object = json!({
            "type": "Object",
            "id": "https://example.org/objects/1",
        });
        let created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let error = sign_object_eddsa(
            &signer_key,
            signer_key_id,
            &object,
            Some(created_at),
            false,
            true, // copy context
            true, // context injection required
        ).err().unwrap();
        assert!(matches!(error, JsonSignatureError::ContextRequired));
    }
}
