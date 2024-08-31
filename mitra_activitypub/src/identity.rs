use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::profiles::types::{
    IdentityProof as DbIdentityProof,
    IdentityProofType,
};
use mitra_utils::{
    did::Did,
    jcs::{
        canonicalize_object,
        CanonicalizationError,
    },
    json_signatures::create::{
        prepare_jcs_sha256_data,
        IntegrityProof,
        IntegrityProofConfig,
    },
};

use super::vocabulary::VERIFIABLE_IDENTITY_STATEMENT;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiableIdentityStatement {
    #[serde(rename = "type")]
    pub object_type: String,

    pub subject: Did,
    pub also_known_as: String,
}

impl VerifiableIdentityStatement {
    fn new(subject: &Did, also_known_as: &str) -> Self {
        Self {
            object_type: VERIFIABLE_IDENTITY_STATEMENT.to_string(),
            subject: subject.clone(),
            also_known_as: also_known_as.to_string(),
        }
    }
}

pub fn create_identity_claim_fep_c390(
    actor_id: &str,
    subject: &Did,
    proof_type: &IdentityProofType,
    proof_created_at: DateTime<Utc>,
) -> Result<(VerifiableIdentityStatement, String), CanonicalizationError> {
    let claim = VerifiableIdentityStatement::new(subject, actor_id);
    let message = match proof_type {
        IdentityProofType::LegacyEip191IdentityProof
            | IdentityProofType::LegacyMinisignIdentityProof
            => unimplemented!("expected FEP-c390 compatible proof type"),
        IdentityProofType::FepC390JcsBlake2Ed25519Proof => {
            subject.as_did_key().expect("did:key should be used");
            canonicalize_object(&claim)?
        },
        IdentityProofType::FepC390JcsEip191Proof => {
            subject.as_did_pkh().expect("did:pkh should be used");
            canonicalize_object(&claim)?
        },
        IdentityProofType::FepC390LegacyJcsEddsaProof => {
            subject.as_did_key().expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa_legacy(
                &subject.to_string(),
                proof_created_at,
            );
            let hash_data = prepare_jcs_sha256_data(&claim, &proof_config)?;
            hex::encode(hash_data)
        },
        IdentityProofType::FepC390EddsaJcsNoCiProof => {
            subject.as_did_key().expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa(
                &subject.to_string(),
                proof_created_at,
                None,
            );
            let hash_data = prepare_jcs_sha256_data(&claim, &proof_config)?;
            hex::encode(hash_data)
        },
    };
    Ok((claim, message))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentityProof {
    #[serde(flatten)]
    statement: VerifiableIdentityStatement,
    proof: IntegrityProof,
}

pub fn create_identity_proof_fep_c390(
    actor_id: &str,
    subject: &Did,
    proof_type: &IdentityProofType,
    proof_created_at: DateTime<Utc>,
    signature_bin: &[u8],
) -> DbIdentityProof {
    let integrity_proof = match proof_type {
        IdentityProofType::LegacyEip191IdentityProof
            | IdentityProofType::LegacyMinisignIdentityProof
            => unimplemented!("expected FEP-c390 compatible proof type"),
        IdentityProofType::FepC390JcsBlake2Ed25519Proof => {
            let did_key = subject.as_did_key()
                .expect("did:key should be used");
            IntegrityProof::jcs_blake2_ed25519(did_key, signature_bin)
        },
        IdentityProofType::FepC390JcsEip191Proof => {
            let did_pkh = subject.as_did_pkh()
                .expect("did:pkh should be used");
            IntegrityProof::jcs_eip191(did_pkh, signature_bin)
        },
        IdentityProofType::FepC390LegacyJcsEddsaProof => {
            let did_key = subject.as_did_key()
                .expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa_legacy(
                &did_key.to_string(),
                proof_created_at,
            );
            IntegrityProof::new(proof_config, signature_bin)
        },
        IdentityProofType::FepC390EddsaJcsNoCiProof => {
            let did_key = subject.as_did_key()
                .expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa(
                &did_key.to_string(),
                proof_created_at,
                None,
            );
            IntegrityProof::new(proof_config, signature_bin)
        },
    };
    let identity_proof = IdentityProof {
        statement: VerifiableIdentityStatement::new(subject, actor_id),
        proof: integrity_proof,
    };
    let proof_value = serde_json::to_value(&identity_proof)
        .expect("identity proof should be serializable");
    DbIdentityProof {
        issuer: identity_proof.statement.subject,
        proof_type: proof_type.clone(),
        value: proof_value,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use mitra_utils::{
        crypto_eddsa::{
            generate_weak_ed25519_key,
            ed25519_public_key_from_secret_key,
            ed25519_secret_key_from_multikey,
        },
        did_key::DidKey,
        json_signatures::{
            create::sign_object_eddsa,
            proofs::ProofType,
            verify::{get_json_signature, verify_eddsa_json_signature},
        },
    };
    use super::*;

    #[test]
    fn test_create_identity_claim_fep_c390() {
        let actor_id = "https://server.example/users/test";
        let ed25519_secret_key = generate_weak_ed25519_key();
        let ed25519_public_key =
            ed25519_public_key_from_secret_key(&ed25519_secret_key);
        let did = Did::Key(DidKey::from_ed25519_key(&ed25519_public_key));
        let created_at = Utc::now();
        let (_claim, message) = create_identity_claim_fep_c390(
            actor_id,
            &did,
            &IdentityProofType::FepC390JcsBlake2Ed25519Proof,
            created_at,
        ).unwrap();
        assert_eq!(
            message,
            r#"{"alsoKnownAs":"https://server.example/users/test","subject":"did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6","type":"VerifiableIdentityStatement"}"#,
        );
    }

    #[test]
    fn test_create_and_verify_identity_proof() {
        // jcs-eddsa-2022; no context injection
        let did_str = "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let did = did_str.parse().unwrap();
        let secret_key_multibase = "z3u2en7t5LR2WtQH5PfFqMqwVHBeXouLzo6haApm8XHqvjxq";
        let secret_key = ed25519_secret_key_from_multikey(secret_key_multibase).unwrap();
        let actor_id = "https://server.example/users/alice";
        let created_at = DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
            .unwrap().with_timezone(&Utc);
        let (claim, _) = create_identity_claim_fep_c390(
            actor_id,
            &did,
            &IdentityProofType::FepC390JcsBlake2Ed25519Proof,
            created_at,
        ).unwrap();
        let claim_value = serde_json::to_value(claim).unwrap();
        let identity_proof = sign_object_eddsa(
            &secret_key,
            &did.to_string(),
            &claim_value,
            Some(created_at),
            true,
            false, // no proof context
        ).unwrap();
        let expected_result = json!({
            "type": "VerifiableIdentityStatement",
            "subject": "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2",
            "alsoKnownAs": "https://server.example/users/alice",
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "jcs-eddsa-2022",
                "verificationMethod": "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2",
                "proofPurpose": "assertionMethod",
                "proofValue": "zYqr4eFzrnUWiBDaa7SmBhfaSBiv6BFRsDRGkmaCJpXArPBspFWNM6NXu77R7JakdzbUdjZihBa28LuWscZxSfRk",
                "created": "2023-02-24T23:36:38Z",
            },
        });
        assert_eq!(identity_proof, expected_result);

        let signature_data = get_json_signature(&identity_proof).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::JcsEddsaSignature,
        );
        let public_key = did.as_did_key().unwrap()
            .try_ed25519_key().unwrap();
        let result = verify_eddsa_json_signature(
            &public_key,
            &signature_data.object,
            &signature_data.proof_config,
            &signature_data.signature,
        );
        assert_eq!(result.is_ok(), true);
    }
}
