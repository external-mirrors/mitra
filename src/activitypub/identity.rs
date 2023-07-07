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
};

use crate::json_signatures::create::IntegrityProof;

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
    use mitra_utils::{
        crypto_eddsa::{
            generate_weak_ed25519_key,
            Ed25519PublicKey,
        },
        currencies::Currency,
        did_key::DidKey,
        did_pkh::DidPkh,
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
        let ed25519_public_key = Ed25519PublicKey::from(&ed25519_private_key);
        let did = Did::Key(
            DidKey::from_ed25519_key(ed25519_public_key.to_bytes()));
        let (_claim, message) = create_identity_claim_fep_c390(
            actor_id,
            &did,
            &IdentityProofType::FepC390JcsBlake2Ed25519Proof,
        ).unwrap();
        assert_eq!(
            message,
            r#"{"alsoKnownAs":"https://server.example/users/test","subject":"did:key:z6MkvTbUjUVTwwMEsqxipAsL9YUvRaAC22rFzQCHf7RnbTbx","type":"VerifiableIdentityStatement"}"#,
        );
    }
}
