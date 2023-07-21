use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use mitra_json_signatures::{
    proofs::{
        ProofType,
        PROOF_TYPE_ID_EIP191,
        PROOF_TYPE_ID_MINISIGN,
    },
    verify::{
        get_json_signature,
        verify_blake2_ed25519_json_signature,
        verify_eddsa_json_signature,
        verify_eip191_json_signature,
        JsonSigner,
    },
};
use mitra_models::{
    database::DatabaseTypeError,
    profiles::types::{
        ExtraField,
        IdentityProof,
        IdentityProofType,
        PaymentLink as DbPaymentLink,
        PaymentOption,
    },
};
use mitra_utils::{
    crypto_eddsa::ed25519_public_key_from_bytes,
    did::Did,
    eip191::verify_eip191_signature,
    minisign::{
        parse_minisign_signature,
        verify_minisign_signature,
    },
};

use crate::activitypub::{
    constants::W3ID_VALUEFLOWS_CONTEXT,
    deserialization::deserialize_string_array,
    identifiers::local_actor_proposal_id,
    identity::{
        create_identity_claim,
        VerifiableIdentityStatement,
    },
    valueflows::builders::PROPOSAL,
    vocabulary::{
        IDENTITY_PROOF,
        LINK,
        PROPERTY_VALUE,
        VERIFIABLE_IDENTITY_STATEMENT,
    },
};
use crate::errors::ValidationError;
use crate::web_client::urls::get_subscription_page_url;

use super::types::ActorAttachment;

pub fn attach_identity_proof(
    proof: IdentityProof,
) -> Result<ActorAttachment, DatabaseTypeError> {
    let proof_type_str = match proof.proof_type {
        IdentityProofType::LegacyEip191IdentityProof => PROOF_TYPE_ID_EIP191,
        IdentityProofType::LegacyMinisignIdentityProof => PROOF_TYPE_ID_MINISIGN,
        _ => unimplemented!("expected legacy identity proof"),
    };
    let proof_value = proof.value.as_str()
        .ok_or(DatabaseTypeError)?
        .to_string();
    let attachment = ActorAttachment {
        object_type: IDENTITY_PROOF.to_string(),
        name: proof.issuer.to_string(),
        value: None,
        href: None,
        signature_algorithm: Some(proof_type_str.to_string()),
        signature_value: Some(proof_value),
    };
    Ok(attachment)
}

pub fn parse_identity_proof(
    actor_id: &str,
    attachment: &JsonValue,
) -> Result<IdentityProof, ValidationError> {
    let attachment: ActorAttachment = serde_json::from_value(attachment.clone())
        .map_err(|_| ValidationError("invalid attachment"))?;
    if attachment.object_type != IDENTITY_PROOF {
        return Err(ValidationError("invalid attachment type"));
    };
    let proof_type_str = attachment.signature_algorithm.as_ref()
        .ok_or(ValidationError("missing proof type"))?;
    let proof_type = match proof_type_str.as_str() {
        PROOF_TYPE_ID_EIP191 => IdentityProofType::LegacyEip191IdentityProof,
        PROOF_TYPE_ID_MINISIGN => IdentityProofType::LegacyMinisignIdentityProof,
        _ => return Err(ValidationError("unsupported proof type")),
    };
    let did = attachment.name.parse::<Did>()
        .map_err(|_| ValidationError("invalid DID"))?;
    let message = create_identity_claim(actor_id, &did)
        .map_err(|_| ValidationError("invalid claim"))?;
    let signature = attachment.signature_value.as_ref()
        .ok_or(ValidationError("missing signature"))?;
    match did {
        Did::Key(ref did_key) => {
            if !matches!(proof_type, IdentityProofType::LegacyMinisignIdentityProof) {
                return Err(ValidationError("incorrect proof type"));
            };
            let signature = parse_minisign_signature(signature)
                .map_err(|_| ValidationError("invalid signature encoding"))?;
            if !signature.is_prehashed {
                return Err(ValidationError("invalid signature type"));
            };
            verify_minisign_signature(
                did_key,
                &message,
                &signature.value,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
        },
        Did::Pkh(ref did_pkh) => {
            if !matches!(proof_type, IdentityProofType::LegacyEip191IdentityProof) {
                return Err(ValidationError("incorrect proof type"));
            };
            let signature_bin = hex::decode(signature)
                .map_err(|_| ValidationError("invalid signature encoding"))?;
            verify_eip191_signature(
                did_pkh,
                &message,
                &signature_bin,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
        },
    };
    let proof_value = serde_json::to_value(signature)
        .expect("signature string should be serializable");
    let proof = IdentityProof {
        issuer: did,
        proof_type: proof_type,
        value: proof_value,
    };
    Ok(proof)
}

pub fn parse_identity_proof_fep_c390(
    actor_id: &str,
    attachment: &JsonValue,
) -> Result<IdentityProof, ValidationError> {
    let statement: VerifiableIdentityStatement = serde_json::from_value(attachment.clone())
        .map_err(|_| ValidationError("invalid FEP-c390 attachment"))?;
    if statement.object_type != VERIFIABLE_IDENTITY_STATEMENT {
        return Err(ValidationError("invalid attachment type"));
    };
    if statement.also_known_as != actor_id {
        return Err(ValidationError("actor ID mismatch"));
    };
    let signature_data = get_json_signature(attachment)
        .map_err(|_| ValidationError("invalid proof"))?;
    let signer = match signature_data.signer {
        JsonSigner::ActorKeyId(_) => return Err(ValidationError("unsupported verification method")),
        JsonSigner::Did(did) => did,
    };
    if signer != statement.subject {
        return Err(ValidationError("subject mismatch"));
    };
    let identity_proof_type = match signature_data.proof_type {
        ProofType::JcsBlake2Ed25519Signature => {
            let did_key = signer.as_did_key()
                .ok_or(ValidationError("unexpected DID type"))?;
            verify_blake2_ed25519_json_signature(
                did_key,
                &signature_data.object,
                &signature_data.signature,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
            IdentityProofType::FepC390JcsBlake2Ed25519Proof
        },
        ProofType::JcsEip191Signature => {
            let did_pkh = signer.as_did_pkh()
                .ok_or(ValidationError("unexpected DID type"))?;
            verify_eip191_json_signature(
                did_pkh,
                &signature_data.object,
                &signature_data.signature,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
            IdentityProofType::FepC390JcsEip191Proof
        },
        ProofType::JcsEddsaSignature => {
            let did_key = signer.as_did_key()
                .ok_or(ValidationError("unexpected DID type"))?;
            let ed25519_key_bytes = did_key.try_ed25519_key()
                .map_err(|_| ValidationError("invalid public key"))?;
            let ed25519_key = ed25519_public_key_from_bytes(&ed25519_key_bytes)
                .map_err(|_| ValidationError("invalid public key"))?;
            verify_eddsa_json_signature(
                &ed25519_key,
                &signature_data.object,
                &signature_data.proof_config,
                &signature_data.signature,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
            IdentityProofType::FepC390JcsEddsaProof
        },
        _ => return Err(ValidationError("unsupported signature type")),
    };
    let proof = IdentityProof {
        issuer: signer,
        proof_type: identity_proof_type,
        value: attachment.clone(),
    };
    Ok(proof)
}

/// https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md
#[derive(Serialize)]
pub struct PaymentLink {
    #[serde(rename = "type")]
    object_type: String,

    pub name: String,
    pub href: String,
    pub rel: Vec<String>,
}

const PAYMENT_LINK_NAME_ETHEREUM: &str = "EthereumSubscription";
const PAYMENT_LINK_NAME_MONERO: &str = "MoneroSubscription";
const PAYMENT_LINK_RELATION_TYPE: &str = "payment";

pub fn attach_payment_option(
    instance_url: &str,
    username: &str,
    payment_option: PaymentOption,
) -> PaymentLink {
    let mut rel = vec![PAYMENT_LINK_RELATION_TYPE.to_string()];
    let (name, href) = match payment_option {
        // Local actors can't have payment links
        PaymentOption::Link(_) => unimplemented!(),
        PaymentOption::EthereumSubscription(_) => {
            let name = PAYMENT_LINK_NAME_ETHEREUM.to_string();
            let href = get_subscription_page_url(instance_url, username);
            (name, href)
        },
        PaymentOption::MoneroSubscription(payment_info) => {
            let name = PAYMENT_LINK_NAME_MONERO.to_string();
            let href = local_actor_proposal_id(
                instance_url,
                username,
                &payment_info.chain_id,
            );
            let proposal_rel = format!("{}{}", W3ID_VALUEFLOWS_CONTEXT, PROPOSAL);
            rel.push(proposal_rel);
            (name, href)
        },
    };
    PaymentLink {
        object_type: LINK.to_string(),
        name: name,
        href: href,
        rel: rel,
    }
}

pub fn parse_payment_option(
    attachment: &JsonValue,
) -> Result<PaymentOption, ValidationError> {
    #[derive(Deserialize)]
    struct PaymentLink {
        name: String,
        href: String,
        #[serde(
            default,
            deserialize_with = "deserialize_string_array",
        )]
        rel: Vec<String>,
    }

    let payment_link: PaymentLink = serde_json::from_value(attachment.clone())
        .map_err(|_| ValidationError("invalid link attachment"))?;
    if payment_link.name != PAYMENT_LINK_NAME_ETHEREUM &&
        payment_link.name != PAYMENT_LINK_NAME_MONERO &&
        !payment_link.rel.contains(&PAYMENT_LINK_RELATION_TYPE.to_string())
    {
        return Err(ValidationError("attachment is not a payment link"));
    };
    let payment_option = PaymentOption::Link(DbPaymentLink {
        name: payment_link.name,
        href: payment_link.href,
    });
    Ok(payment_option)
}

pub fn attach_extra_field(
    field: ExtraField,
) -> ActorAttachment {
    ActorAttachment {
        object_type: PROPERTY_VALUE.to_string(),
        name: field.name,
        value: Some(field.value),
        href: None,
        signature_algorithm: None,
        signature_value: None,
    }
}

pub fn parse_property_value(
    attachment: &JsonValue,
) -> Result<ExtraField, ValidationError> {
    let attachment: ActorAttachment = serde_json::from_value(attachment.clone())
        .map_err(|_| ValidationError("invalid attachment"))?;
    if attachment.object_type != PROPERTY_VALUE {
        return Err(ValidationError("invalid attachment type"));
    };
    let property_value = attachment.value.as_ref()
        .ok_or(ValidationError("missing property value"))?;
    let field = ExtraField {
        name: attachment.name.clone(),
        value: property_value.to_string(),
        value_source: None,
    };
    Ok(field)
}

/// https://codeberg.org/fediverse/fep/src/commit/391099a97cd1ad9388e83ffff8ed1f7be5203b7b/feps/fep-fb2a.md
pub fn parse_metadata_field(
    attachment: &JsonValue,
) -> Result<ExtraField, ValidationError> {
    #[derive(Deserialize)]
    struct Note {
        name: String,
        content: String,
    }

    let note: Note = serde_json::from_value(attachment.clone())
        .map_err(|_| ValidationError("invalid metadata field"))?;
    let field = ExtraField {
        name: note.name,
        value: note.content,
        value_source: None,
    };
    Ok(field)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use mitra_utils::{
        caip2::ChainId,
        currencies::Currency,
        did::Did,
        did_pkh::DidPkh,
    };
    use crate::activitypub::identity::{
        create_identity_claim_fep_c390,
        create_identity_proof_fep_c390,
    };
    use crate::ethereum::{
        signatures::{generate_ecdsa_key, sign_message},
        utils::{address_to_string, key_to_ethereum_address},
    };
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_identity_proof_fep_c390() {
        let actor_id = "https://server.example/users/test";
        let private_key = generate_ecdsa_key();
        let address = address_to_string(key_to_ethereum_address(&private_key));
        let did_pkh = DidPkh::from_address(&Currency::Ethereum, &address);
        let did = Did::Pkh(did_pkh);
        let proof_type = IdentityProofType::FepC390JcsEip191Proof;
        let created_at = Utc::now();
        let (_claim, message) = create_identity_claim_fep_c390(
            actor_id,
            &did,
            &proof_type,
            created_at,
        ).unwrap();
        let signature = sign_message(
            &private_key.display_secret().to_string(),
            message.as_bytes(),
        ).unwrap();
        let signature_bin = signature.to_bytes();
        let identity_proof = create_identity_proof_fep_c390(
            actor_id,
            &did,
            &proof_type,
            created_at,
            &signature_bin,
        );
        let parsed = parse_identity_proof_fep_c390(
            actor_id,
            &identity_proof.value,
        ).unwrap();
        assert_eq!(parsed.issuer, identity_proof.issuer);
        assert_eq!(parsed.proof_type, identity_proof.proof_type);
        assert_eq!(parsed.value, identity_proof.value);
    }

    #[test]
    fn test_extra_field() {
        let field = ExtraField {
            name: "test".to_string(),
            value: "value".to_string(),
            value_source: None,
        };
        let attachment = attach_extra_field(field.clone());
        assert_eq!(attachment.object_type, PROPERTY_VALUE);

        let attachment_value = serde_json::to_value(attachment).unwrap();
        let parsed_field = parse_property_value(&attachment_value).unwrap();
        assert_eq!(parsed_field.name, field.name);
        assert_eq!(parsed_field.value, field.value);
    }

    #[test]
    fn test_payment_option() {
        let username = "testuser";
        let price = 240000;
        let payout_address = "test";
        let payment_option = PaymentOption::monero_subscription(
            ChainId::monero_mainnet(),
            price,
            payout_address.to_string(),
        );
        let subscription_page_url =
            "https://example.com/users/testuser/proposals/monero:418015bb9ae982a1975da7d79277c270";
        let attachment = attach_payment_option(
            INSTANCE_URL,
            username,
            payment_option,
        );
        assert_eq!(attachment.object_type, LINK);
        assert_eq!(attachment.name, "MoneroSubscription");
        assert_eq!(attachment.href, subscription_page_url);
        assert_eq!(attachment.rel.len(), 2);
        assert_eq!(attachment.rel[0], "payment");
        assert_eq!(attachment.rel[1], "https://w3id.org/valueflows/Proposal");

        let attachment_value = serde_json::to_value(attachment).unwrap();
        let parsed_option = parse_payment_option(&attachment_value).unwrap();
        let link = match parsed_option {
            PaymentOption::Link(link) => link,
            _ => panic!("wrong option"),
        };
        assert_eq!(link.name, "MoneroSubscription");
        assert_eq!(link.href, subscription_page_url);
    }

    #[test]
    fn test_parse_link_attachment_not_payment() {
        let attachment_value = json!({
            "name": "Test",
            "href": "https://test.example",
        });
        let error = parse_payment_option(&attachment_value).err().unwrap();
        assert!(matches!(
            error,
            ValidationError("attachment is not a payment link"),
        ));
    }
}
