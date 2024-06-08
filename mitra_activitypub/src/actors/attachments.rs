use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use mitra_federation::{
    constants::AP_MEDIA_TYPE,
    deserialization::deserialize_string_array,
};
use mitra_models::{
    profiles::types::{
        ExtraField,
        IdentityProof,
        IdentityProofType,
        PaymentLink as DbPaymentLink,
        PaymentOption,
    },
};
use mitra_validators::profiles::validate_extra_field;
use mitra_utils::{
    json_signatures::{
        proofs::{
            ProofType,
        },
        verify::{
            get_json_signature,
            verify_blake2_ed25519_json_signature,
            verify_eddsa_json_signature,
            verify_eip191_json_signature,
            JsonSigner,
        },
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    authority::Authority,
    constants::{
        CHAT_LINK_RELATION_TYPE,
        PAYMENT_LINK_RELATION_TYPE,
    },
    contexts::W3ID_VALUEFLOWS_CONTEXT,
    identifiers::{local_actor_id_unified, local_actor_proposal_id},
    identity::VerifiableIdentityStatement,
    vocabulary::{
        LINK,
        PROPERTY_VALUE,
        PROPOSAL,
        VERIFIABLE_IDENTITY_STATEMENT,
    },
};

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
            let ed25519_key = did_key.try_ed25519_key()
                .map_err(|_| ValidationError("invalid public key"))?;
            verify_eddsa_json_signature(
                &ed25519_key,
                &signature_data.object,
                &signature_data.proof_config,
                &signature_data.signature,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
            IdentityProofType::FepC390LegacyJcsEddsaProof
        },
        ProofType::EddsaJcsSignature => {
            // eddsa-jcs-2022 identity proofs are temporarily rejected
            return Err(ValidationError("eddsa-jcs-2022 cryptosuite is not supported"));
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
#[serde(rename_all = "camelCase")]
pub struct PaymentLink {
    #[serde(rename = "type")]
    object_type: String,

    pub name: String,
    pub href: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    pub rel: Vec<String>,
}

const PAYMENT_LINK_NAME_ETHEREUM: &str = "EthereumSubscription";
const PAYMENT_LINK_NAME_MONERO: &str = "MoneroSubscription";

// TODO: remove
fn valueflows_proposal_rel_legacy() -> String {
    format!("{}{}", "https://w3id.org/valueflows/", PROPOSAL)
}

fn valueflows_proposal_rel() -> String {
    format!("{}#{}", W3ID_VALUEFLOWS_CONTEXT, PROPOSAL)
}

pub fn attach_payment_option(
    authority: &Authority,
    username: &str,
    payment_option: PaymentOption,
) -> PaymentLink {
    let mut rel = vec![PAYMENT_LINK_RELATION_TYPE.to_string()];
    let (name, href) = match payment_option {
        // Local actors can't have payment links
        PaymentOption::Link(_) => unimplemented!(),
        PaymentOption::EthereumSubscription(payment_info) => {
            let name = PAYMENT_LINK_NAME_ETHEREUM.to_string();
            let actor_id = local_actor_id_unified(authority, username);
            let href = local_actor_proposal_id(
                &actor_id,
                &payment_info.chain_id,
            );
            (name, href)
        },
        PaymentOption::MoneroSubscription(payment_info) => {
            let name = PAYMENT_LINK_NAME_MONERO.to_string();
            let actor_id = local_actor_id_unified(authority, username);
            let href = local_actor_proposal_id(
                &actor_id,
                &payment_info.chain_id,
            );
            rel.push(valueflows_proposal_rel_legacy());
            (name, href)
        },
        PaymentOption::RemoteMoneroSubscription(_) => unimplemented!(),
    };
    PaymentLink {
        object_type: LINK.to_string(),
        name: name,
        href: href,
        media_type: Some(AP_MEDIA_TYPE.to_string()),
        rel: rel,
    }
}

pub enum LinkAttachment {
    PaymentLink(DbPaymentLink),
    Proposal(DbPaymentLink),
    ChatLink(ExtraField),
}

/// https://codeberg.org/fediverse/fep/src/branch/main/fep/fb2a/fep-fb2a.md
pub fn parse_link(
    attachment: &JsonValue,
) -> Result<LinkAttachment, ValidationError> {
    #[derive(Deserialize)]
    struct Link {
        name: String,
        href: String,
        #[serde(
            default,
            deserialize_with = "deserialize_string_array",
        )]
        rel: Vec<String>,
    }

    let link: Link = serde_json::from_value(attachment.clone())
        .map_err(|_| ValidationError("invalid link attachment"))?;
    let result = if link.rel.contains(&PAYMENT_LINK_RELATION_TYPE.to_string()) {
        let db_payment_link = DbPaymentLink {
            name: link.name,
            href: link.href,
        };
        if link.rel.contains(&valueflows_proposal_rel_legacy()) ||
            link.rel.contains(&valueflows_proposal_rel())
        {
            LinkAttachment::Proposal(db_payment_link)
        } else {
            LinkAttachment::PaymentLink(db_payment_link)
        }
    } else if link.rel.contains(&CHAT_LINK_RELATION_TYPE.to_string()) {
        // https://codeberg.org/fediverse/fep/src/branch/main/fep/1970/fep-1970.md
        let field = ExtraField {
            name: link.name,
            value: link.href,
            value_source: None,
        };
        LinkAttachment::ChatLink(field)
    } else {
        return Err(ValidationError("unknown link type"));
    };
    Ok(result)
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyValue {
    #[serde(rename = "type")]
    object_type: String,

    name: String,
    value: String,
}

pub fn attach_extra_field(
    field: ExtraField,
) -> PropertyValue {
    PropertyValue {
        object_type: PROPERTY_VALUE.to_string(),
        name: field.name,
        value: field.value,
    }
}

pub fn parse_property_value(
    attachment: &JsonValue,
) -> Result<ExtraField, ValidationError> {
    let attachment: PropertyValue = serde_json::from_value(attachment.clone())
        .map_err(|_| ValidationError("invalid attachment"))?;
    if attachment.object_type != PROPERTY_VALUE {
        return Err(ValidationError("invalid attachment type"));
    };
    let field = ExtraField {
        name: attachment.name,
        value: attachment.value,
        value_source: None,
    };
    validate_extra_field(&field)?;
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
    validate_extra_field(&field)?;
    Ok(field)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use chrono::Utc;
    use serde_json::json;
    use mitra_utils::{
        caip2::ChainId,
        crypto_ecdsa::generate_ecdsa_key,
        currencies::Currency,
        did::Did,
        did_pkh::DidPkh,
        eip191::{create_eip191_signature, ecdsa_public_key_to_address_hex},
    };
    use crate::identity::{
        create_identity_claim_fep_c390,
        create_identity_proof_fep_c390,
    };
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_identity_proof_fep_c390() {
        let actor_id = "https://server.example/users/test";
        let secret_key = generate_ecdsa_key();
        let address = ecdsa_public_key_to_address_hex(&secret_key.verifying_key());
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
        let signature = create_eip191_signature(
            &secret_key,
            message.as_bytes(),
        ).unwrap();
        let identity_proof = create_identity_proof_fep_c390(
            actor_id,
            &did,
            &proof_type,
            created_at,
            &signature,
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
        let authority = Authority::server(INSTANCE_URL);
        let username = "testuser";
        let price = NonZeroU64::new(240000).unwrap();
        let payout_address = "test";
        let payment_option = PaymentOption::monero_subscription(
            ChainId::monero_mainnet(),
            price,
            payout_address.to_string(),
        );
        let subscription_page_url =
            "https://example.com/users/testuser/proposals/monero:418015bb9ae982a1975da7d79277c270";
        let attachment = attach_payment_option(
            &authority,
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
        let attachment = parse_link(&attachment_value).unwrap();
        let payment_link = match attachment {
            LinkAttachment::Proposal(payment_link) => payment_link,
            _ => panic!("not a proposal"),
        };
        assert_eq!(payment_link.name, "MoneroSubscription");
        assert_eq!(payment_link.href, subscription_page_url);
    }

    #[test]
    fn test_parse_link_attachment_unknown() {
        let attachment_value = json!({
            "name": "Test",
            "href": "https://test.example",
        });
        let error = parse_link(&attachment_value).err().unwrap();
        assert!(matches!(
            error,
            ValidationError("unknown link type"),
        ));
    }
}
