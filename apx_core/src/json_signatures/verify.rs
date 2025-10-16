//! Verify JSON signatures
use std::fmt;

use serde_json::{Value as JsonValue};
use thiserror::Error;

use crate::{
    crypto::{
        eddsa::{verify_eddsa_signature, Ed25519PublicKey},
        rsa::{verify_rsa_sha256_signature, RsaPublicKey},
    },
    did_url::DidUrl,
    jcs::{
        canonicalize_object,
        CanonicalizationError,
    },
    multibase::{decode_multibase_base58btc, MultibaseError},
    url::{
        ap_uri::{is_ap_uri, ApUri},
        common::Origin,
        http_uri::HttpUri,
    },
};

use super::create::{
    prepare_jcs_sha256_data,
    IntegrityProofConfig,
    LD_SIGNATURE_KEY,
    PROOF_KEY,
    PURPOSE_ASSERTION_METHOD,
    PURPOSE_AUTHENTICATION,
};
use super::proofs::{ProofType, DATA_INTEGRITY_PROOF};

#[cfg(feature = "eip191")]
use crate::{
    did_pkh::DidPkh,
    eip191::verify_eip191_signature,
};

#[cfg(feature = "minisign")]
use crate::{
    did_key::DidKey,
    minisign::verify_minisign_signature,
};

const PROOF_VALUE_KEY: &str = "proofValue";

/// Signature verification method
#[derive(Debug, PartialEq)]
pub enum VerificationMethod {
    HttpUri(HttpUri),
    ApUri(ApUri),
    DidUrl(DidUrl),
}

impl VerificationMethod {
    /// Parses verification method ID
    pub(crate) fn parse(url: &str) -> Result<Self, &'static str> {
        // TODO: support compatible 'ap' URIs
        let method = if is_ap_uri(url) {
            let ap_uri = ApUri::parse(url)?;
            Self::ApUri(ap_uri)
        } else if let Ok(did_url) = DidUrl::parse(url) {
            Self::DidUrl(did_url)
        } else if let Ok(http_uri) = HttpUri::parse(url) {
            Self::HttpUri(http_uri)
        } else {
            return Err("invalid verification method ID");
        };
        Ok(method)
    }

    /// Returns origin tuple for this verification method
    pub fn origin(&self) -> Origin {
        match self {
            Self::HttpUri(http_uri) => http_uri.origin(),
            Self::ApUri(ap_uri) => ap_uri.origin(),
            Self::DidUrl(did_url) => did_url.origin(),
        }
    }
}

impl fmt::Display for VerificationMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HttpUri(http_uri) => write!(formatter, "{}", http_uri),
            Self::ApUri(ap_uri) => write!(formatter, "{}", ap_uri),
            Self::DidUrl(did_url) => write!(formatter, "{}", did_url),
        }
    }
}

/// Parsed integrity proof
pub struct JsonSignatureData {
    pub proof_type: ProofType,
    pub verification_method: VerificationMethod,
    pub object: JsonValue,
    pub proof_config: JsonValue,
    pub signature: Vec<u8>,
}

/// Errors that may occur during the verification of a JSON signature
#[derive(Debug, Error)]
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

/// Parses integrity proof on a JSON document
pub fn get_json_signature(
    object: &JsonValue,
) -> Result<JsonSignatureData, VerificationError> {
    let mut object = object.clone();
    let object_map = object.as_object_mut()
        .ok_or(VerificationError::InvalidObject)?;
    // If linked data signature is present,
    // it must be removed before verification (per FEP-8b32)
    object_map.remove(LD_SIGNATURE_KEY);
    let mut proof = object_map.remove(PROOF_KEY)
        .ok_or(VerificationError::NoProof)?;
    if let Some(context) = proof.get("@context") {
        if *context != object["@context"] {
            return Err(VerificationError::InvalidProof("incorrect proof context"));
        };
    };
    let proof_value = proof.as_object_mut()
        .ok_or(VerificationError::InvalidProof("invalid proof"))?
        .remove(PROOF_VALUE_KEY)
        .ok_or(VerificationError::InvalidProof("'proofValue' is missing"))?
        .as_str()
        .ok_or(VerificationError::InvalidProof("invalid proof value"))?
        .to_string();
    let proof_config: IntegrityProofConfig = serde_json::from_value(proof.clone())
        .map_err(|_| VerificationError::InvalidProof("invalid proof configuration"))?;
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
    let verification_method = VerificationMethod::parse(&proof_config.verification_method)
        .map_err(VerificationError::InvalidProof)?;
    let signature = decode_multibase_base58btc(&proof_value)?;
    let signature_data = JsonSignatureData {
        proof_type,
        verification_method,
        object,
        proof_config: proof,
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
    verify_rsa_sha256_signature(
        signer_key,
        canonical_object.as_bytes(),
        signature,
    ).map_err(|_| VerificationError::InvalidSignature)?;
    Ok(())
}

pub fn verify_eddsa_json_signature(
    signer_key: &Ed25519PublicKey,
    object: &JsonValue,
    proof_config: &JsonValue,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let hash_data = prepare_jcs_sha256_data(object, proof_config)?;
    verify_eddsa_signature(
        signer_key,
        &hash_data,
        signature,
    ).map_err(|_| VerificationError::InvalidSignature)?;
    Ok(())
}

#[cfg(feature = "eip191")]
pub fn verify_eip191_json_signature(
    signer: &DidPkh,
    object: &JsonValue,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let canonical_object = canonicalize_object(object)?;
    verify_eip191_signature(signer, &canonical_object, signature)
        .map_err(|_| VerificationError::InvalidSignature)
}

#[cfg(feature = "minisign")]
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
    use crate::{
        crypto::{
            eddsa::{
                generate_ed25519_key,
                ed25519_public_key_from_multikey,
                ed25519_public_key_from_secret_key,
                ed25519_secret_key_from_multikey,
            },
            rsa::generate_weak_rsa_key,
        },
        json_signatures::create::{
            sign_object,
            sign_object_eddsa,
        },
    };
    use super::*;

    #[allow(deprecated)]
    use crate::json_signatures::create::sign_object_rsa;

    #[cfg(feature = "eip191")]
    use crate::did::Did;

    #[test]
    fn test_verification_method_parse() {
        let url = "http://social.example/actors/1#main-key";
        let vm_id = VerificationMethod::parse(url).unwrap();
        assert!(matches!(vm_id, VerificationMethod::HttpUri(_)));

        let url = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key";
        let vm_id = VerificationMethod::parse(url).unwrap();
        assert!(matches!(vm_id, VerificationMethod::ApUri(_)));

        let url = "https://gateway.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor#main-key";
        let vm_id = VerificationMethod::parse(url).unwrap();
        assert!(matches!(vm_id, VerificationMethod::HttpUri(_)));

        let url = "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2#z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let vm_id = VerificationMethod::parse(url).unwrap();
        assert!(matches!(vm_id, VerificationMethod::DidUrl(_)));
    }

    #[cfg(feature = "eip191")]
    #[test]
    fn test_get_json_signature_eip191() {
        let signed_object = json!({
            "type": "Test",
            "id": "https://example.org/objects/1",
            "proof": {
                "type": "MitraJcsEip191Signature2022",
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
        let expected_did = Did::Pkh(DidPkh::from_ethereum_address(
            "0xb9c5714089478a327f09197987f16f9e5d936e8a"));
        let did_url = match signature_data.verification_method {
            VerificationMethod::DidUrl(did_url) => did_url,
            _ => panic!("unexpected verification method"),
        };
        assert_eq!(did_url.did(), &expected_did);
        assert_eq!(signature_data.signature, [171, 205]);
    }

    #[test]
    #[allow(deprecated)]
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
        let expected_vm =
            VerificationMethod::HttpUri(HttpUri::parse(signer_key_id).unwrap());
        assert_eq!(signature_data.verification_method, expected_vm);

        let signer_public_key = RsaPublicKey::from(signer_key);
        let result = verify_rsa_json_signature(
            &signer_public_key,
            &signature_data.object,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    #[allow(deprecated)]
    fn test_create_and_verify_eddsa_signature_legacy() {
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
            true,
            false,
            false,
        ).unwrap();

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::JcsEddsaSignature,
        );
        let expected_vm =
            VerificationMethod::HttpUri(HttpUri::parse(signer_key_id).unwrap());
        assert_eq!(signature_data.verification_method, expected_vm);

        let signer_public_key =
            ed25519_public_key_from_secret_key(&signer_key);
        let result = verify_eddsa_json_signature(
            &signer_public_key,
            &signature_data.object,
            &signature_data.proof_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_eddsa_signature() {
        let signer_key = generate_ed25519_key();
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
        let signed_object = sign_object(
            &signer_key,
            signer_key_id,
            &object,
        ).unwrap();

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::EddsaJcsSignature,
        );
        let expected_vm =
            VerificationMethod::HttpUri(HttpUri::parse(signer_key_id).unwrap());
        assert_eq!(signature_data.verification_method, expected_vm);

        let signer_public_key =
            ed25519_public_key_from_secret_key(&signer_key);
        let result = verify_eddsa_json_signature(
            &signer_public_key,
            &signature_data.object,
            &signature_data.proof_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_eddsa_signature_fep_8b32_test_vector() {
        // https://codeberg.org/fediverse/fep/src/branch/main/fep/8b32/fep-8b32.feature
        let secret_key_multibase = "z3u2en7t5LR2WtQH5PfFqMqwVHBeXouLzo6haApm8XHqvjxq";
        let secret_key = ed25519_secret_key_from_multikey(secret_key_multibase).unwrap();
        let key_id = "https://server.example/users/alice#ed25519-key";
        let created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let object = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/data-integrity/v2"
            ],
            "id": "https://server.example/activities/1",
            "type": "Create",
            "actor": "https://server.example/users/alice",
            "object": {
                "id": "https://server.example/objects/1",
                "type": "Note",
                "attributedTo": "https://server.example/users/alice",
                "content": "Hello world",
                "location": {
                    "type": "Place",
                    "longitude": -71.184902,
                    "latitude": 25.273962
                }
            }
        });
        let signed_object = sign_object_eddsa(
            &secret_key,
            key_id,
            &object,
            Some(created_at),
            false,
            true, // with proof @context
            false,
        ).unwrap();

        let expected_result = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/data-integrity/v2"
            ],
            "id": "https://server.example/activities/1",
            "type": "Create",
            "actor": "https://server.example/users/alice",
            "object": {
                "id": "https://server.example/objects/1",
                "type": "Note",
                "attributedTo": "https://server.example/users/alice",
                "content": "Hello world",
                "location": {
                    "type": "Place",
                    "longitude": -71.184902,
                    "latitude": 25.273962
                }
            },
            "proof": {
                "@context": [
                    "https://www.w3.org/ns/activitystreams",
                    "https://w3id.org/security/data-integrity/v2"
                ],
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-jcs-2022",
                "verificationMethod": "https://server.example/users/alice#ed25519-key",
                "proofPurpose": "assertionMethod",
                "proofValue": "z42ffGu6AUKPCFcFPiabmUvnGLPJzC7e4DGWC52NUasSSH37UMa9c58tdgVszUcZfytxa4fQ5TYHaJENCxUDe9SdL",
                "created": "2023-02-24T23:36:38Z"
            }
        });
        assert_eq!(signed_object, expected_result);

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::EddsaJcsSignature,
        );
        let public_key_multibase = "z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let public_key = ed25519_public_key_from_multikey(public_key_multibase).unwrap();
        let result = verify_eddsa_json_signature(
            &public_key,
            &signature_data.object,
            &signature_data.proof_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_eddsa_signature_vc_di_eddsa_test_vector() {
        // https://w3c.github.io/vc-di-eddsa/#representation-eddsa-jcs-2022
        let secret_key_multibase = "z3u2en7t5LR2WtQH5PfFqMqwVHBeXouLzo6haApm8XHqvjxq";
        let secret_key = ed25519_secret_key_from_multikey(secret_key_multibase).unwrap();
        let key_id = "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2#z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let object = json!({
            "@context": [
                "https://www.w3.org/ns/credentials/v2",
                "https://www.w3.org/ns/credentials/examples/v2"
            ],
            "id": "urn:uuid:58172aac-d8ba-11ed-83dd-0b3aef56cc33",
            "type": ["VerifiableCredential", "AlumniCredential"],
            "name": "Alumni Credential",
            "description": "A minimum viable example of an Alumni Credential.",
            "issuer": "https://vc.example/issuers/5678",
            "validFrom": "2023-01-01T00:00:00Z",
            "credentialSubject": {
                "id": "did:example:abcdefgh",
                "alumniOf": "The School of Examples"
            }
        });
        let signed_object = sign_object_eddsa(
            &secret_key,
            key_id,
            &object,
            Some(created_at),
            false,
            true, // with proof context
            true, // context injection required
        ).unwrap();

        let expected_result = json!({
            "@context": [
                "https://www.w3.org/ns/credentials/v2",
                "https://www.w3.org/ns/credentials/examples/v2"
            ],
            "id": "urn:uuid:58172aac-d8ba-11ed-83dd-0b3aef56cc33",
            "type": [
                "VerifiableCredential",
                "AlumniCredential"
            ],
            "name": "Alumni Credential",
            "description": "A minimum viable example of an Alumni Credential.",
            "issuer": "https://vc.example/issuers/5678",
            "validFrom": "2023-01-01T00:00:00Z",
            "credentialSubject": {
                "id": "did:example:abcdefgh",
                "alumniOf": "The School of Examples"
            },
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-jcs-2022",
                "created": "2023-02-24T23:36:38Z",
                "verificationMethod": "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2#z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2",
                "proofPurpose": "assertionMethod",
                "@context": [
                    "https://www.w3.org/ns/credentials/v2",
                    "https://www.w3.org/ns/credentials/examples/v2"
                ],
                "proofValue": "z2HnFSSPPBzR36zdDgK8PbEHeXbR56YF24jwMpt3R1eHXQzJDMWS93FCzpvJpwTWd3GAVFuUfjoJdcnTMuVor51aX"
            }
        });
        assert_eq!(signed_object, expected_result);

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::EddsaJcsSignature,
        );
        let public_key_multibase = "z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let public_key = ed25519_public_key_from_multikey(public_key_multibase).unwrap();
        let result = verify_eddsa_json_signature(
            &public_key,
            &signature_data.object,
            &signature_data.proof_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
