use serde_json::{Value as JsonValue};
use thiserror::Error;

use apx_core::{
    json_signatures::{
        proofs::ProofType,
        verify::{
            get_json_signature,
            verify_eddsa_json_signature,
            JsonSignatureVerificationError as JsonSignatureError,
            VerificationMethod,
        },
    },
    url::canonical::Url,
};

#[derive(Debug, Error)]
pub enum AuthenticationError {
    #[error("{0}")]
    InvalidObjectID(&'static str),

    #[error("object is not portable")]
    NotPortable,

    #[error("no proof")]
    NoProof,

    #[error("invalid verification method")]
    InvalidVerificationMethod,

    #[error("owner and object signer do not match")]
    UnexpectedSigner,

    #[error("unexpected proof type")]
    UnexpectedProofType,

    #[error(transparent)]
    JsonSignatureError(#[from] JsonSignatureError),
}

pub fn verify_portable_object(
    object: &JsonValue,
) -> Result<String, AuthenticationError> {
    let object_id = object["id"].as_str()
        .ok_or(AuthenticationError::InvalidObjectID("'id' property not found"))?;
    let canonical_object_id = Url::parse(object_id)
        .map_err(|error| AuthenticationError::InvalidObjectID(error.0))?;
    let canonical_object_id = match canonical_object_id {
        // Only portable objects must have an integrity proof
        Url::Http(_) => return Err(AuthenticationError::NotPortable),
        Url::Ap(ap_url) => ap_url,
    };
    let signature_data = match get_json_signature(object) {
        Ok(signature_data) => signature_data,
        Err(JsonSignatureError::NoProof) => return Err(AuthenticationError::NoProof),
        Err(other_error) => return Err(other_error.into()),
    };
    match signature_data.verification_method {
        VerificationMethod::HttpUrl(_) =>
            return Err(AuthenticationError::InvalidVerificationMethod),
        VerificationMethod::DidUrl(did_url) => {
            // Object must be signed by its owner
            if did_url.did() != canonical_object_id.authority() {
                return Err(AuthenticationError::UnexpectedSigner);
            };
            // DID URL fragment is ignored because supported DIDs
            // can't have more than one verification method
            let did = did_url.did();
            match signature_data.proof_type {
                ProofType::EddsaJcsSignature => {
                    let signer_key = did.as_did_key()
                        .ok_or(AuthenticationError::InvalidVerificationMethod)?
                        .try_ed25519_key()
                        .map_err(|_| AuthenticationError::InvalidVerificationMethod)?;
                    verify_eddsa_json_signature(
                        &signer_key,
                        &signature_data.object,
                        &signature_data.proof_config,
                        &signature_data.signature,
                    )?;
                },
                _ => return Err(AuthenticationError::UnexpectedProofType),
            };
        },
    };
    Ok(object_id.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_verify_portable_object() {
        let object = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/data-integrity/v1",
            ],
            "type": "Note",
            "attributedTo": "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "id": "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/testobject",
            "content": "test",
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-jcs-2022",
                "created": "2023-02-24T23:36:38Z",
                "verificationMethod": "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
                "proofPurpose": "assertionMethod",
                "proofValue": "z5kjVLvxaFQ4WpdCcM1RbkGqruFUTtYgX89XynSQjH4DYEVUWQhCVKLMRuTByYWqQS8SmxSJKeiBh9f2Y84pbfemn",
            },
        });
        let result = verify_portable_object(&object);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_portable_object_not_portable() {
        let object = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/data-integrity/v1",
            ],
            "type": "Note",
            "attributedTo": "https://social.example/actor",
            "id": "https://social.example/testobject",
            "content": "test",
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-jcs-2022",
                "created": "2023-02-24T23:36:38Z",
                "verificationMethod": "did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
                "proofPurpose": "assertionMethod",
                "proofValue": "z2h4dRMiJ81tjdgBGAYw2EzASUWVMcEXBfod2T9mg2YMYCHAP4ehHjdT6nPAFJbpugAkG4bN7xcN8BvyVSxk4f79U",
            },
        });
        let result = verify_portable_object(&object);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().to_string(), "object is not portable");
    }
}
