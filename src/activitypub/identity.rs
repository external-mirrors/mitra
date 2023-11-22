use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::profiles::types::{
    IdentityProof as DbIdentityProof,
    IdentityProofType,
};
use mitra_utils::{
    canonicalization::{
        canonicalize_object,
        CanonicalizationError,
    },
    did::Did,
    json_signatures::create::{
        prepare_jcs_sha256_data,
        IntegrityProof,
        IntegrityProofConfig,
    },
};

use super::vocabulary::VERIFIABLE_IDENTITY_STATEMENT;

// https://www.w3.org/TR/vc-data-model/#credential-subject
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Claim {
    id: String, // actor ID
    owner_of: Did,
}

/// Creates key ownership claim and prepares it for signing
pub fn create_identity_claim(
    actor_id: &str,
    did: &Did,
) -> Result<String, CanonicalizationError> {
    let claim = Claim {
        id: actor_id.to_string(),
        owner_of: did.clone(),
    };
    let message = canonicalize_object(&claim)?;
    Ok(message)
}

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
        IdentityProofType::LegacyFepC390JcsEddsaProof => {
            subject.as_did_key().expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa_legacy(
                &subject.to_string(),
                proof_created_at,
            );
            let hash_data = prepare_jcs_sha256_data(&claim, &proof_config)?;
            hex::encode(hash_data)
        },
        IdentityProofType::FepC390JcsEddsaProof => {
            subject.as_did_key().expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa(
                &subject.to_string(),
                proof_created_at,
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
        IdentityProofType::LegacyFepC390JcsEddsaProof => {
            let did_key = subject.as_did_key()
                .expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa_legacy(
                &did_key.to_string(),
                proof_created_at,
            );
            IntegrityProof::new(proof_config, signature_bin)
        },
        IdentityProofType::FepC390JcsEddsaProof => {
            let did_key = subject.as_did_key()
                .expect("did:key should be used");
            let proof_config = IntegrityProofConfig::jcs_eddsa(
                &did_key.to_string(),
                proof_created_at,
            );
            IntegrityProof::new(proof_config, signature_bin)
        },
        _ => unimplemented!("expected FEP-c390 compatible proof type"),
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
            ed25519_private_key_from_bytes,
            ed25519_public_key_from_bytes,
            ed25519_public_key_from_private_key,
        },
        currencies::Currency,
        did_key::DidKey,
        did_pkh::DidPkh,
        json_signatures::{
            create::sign_object_eddsa,
            proofs::ProofType,
            verify::{get_json_signature, verify_eddsa_json_signature},
        },
        multibase::decode_multibase_base58btc,
        multicodec::decode_ed25519_private_key,
    };
    use super::*;

    #[test]
    fn test_create_identity_claim() {
        let actor_id = "https://server.example/users/test";
        let ethereum_address = "0xB9C5714089478a327F09197987f16f9E5d936E8a";
        let did = Did::Pkh(DidPkh::from_address(
            &Currency::Ethereum,
            ethereum_address,
        ));
        let claim = create_identity_claim(actor_id, &did).unwrap();
        assert_eq!(
            claim,
            r#"{"id":"https://server.example/users/test","ownerOf":"did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a"}"#,
        );
    }

    #[test]
    fn test_create_identity_claim_fep_c390() {
        let actor_id = "https://server.example/users/test";
        let ed25519_private_key = generate_weak_ed25519_key();
        let ed25519_public_key =
            ed25519_public_key_from_private_key(&ed25519_private_key);
        let did = Did::Key(
            DidKey::from_ed25519_key(ed25519_public_key.to_bytes()));
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
    fn test_create_and_verify_identity_proof_test_vector() {
        let did_str = "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
        let did = did_str.parse().unwrap();
        let private_key_multibase = "z3u2en7t5LR2WtQH5PfFqMqwVHBeXouLzo6haApm8XHqvjxq";
        let private_key_multicode = decode_multibase_base58btc(private_key_multibase).unwrap();
        let private_key_bytes = decode_ed25519_private_key(&private_key_multicode).unwrap();
        let private_key = ed25519_private_key_from_bytes(&private_key_bytes).unwrap();
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
            &private_key,
            &did.to_string(),
            &claim_value,
            Some(created_at),
        ).unwrap();
        let expected_result = json!({
            "type": "VerifiableIdentityStatement",
            "subject": "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2",
            "alsoKnownAs": "https://server.example/users/alice",
            "proof": {
                "type": "DataIntegrityProof",
                "cryptosuite": "eddsa-jcs-2022",
                "verificationMethod": "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2",
                "proofPurpose": "assertionMethod",
                "proofValue": "z26W7TfJYD9DrGqnem245zNbeCbTwjb8avpduzi1JPhFrwML99CpP6gGXSKSXAcQdpGFBXF4kx7VwtXKhu7VDZJ54",
                "created": "2023-02-24T23:36:38Z",
            },
        });
        assert_eq!(identity_proof, expected_result);

        let signature_data = get_json_signature(&identity_proof).unwrap();
        assert_eq!(
            signature_data.proof_type,
            ProofType::JcsEddsaSignature,
        );
        let public_key_bytes = did.as_did_key().unwrap()
            .try_ed25519_key().unwrap();
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
