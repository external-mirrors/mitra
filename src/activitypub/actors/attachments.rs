use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use mitra_models::profiles::types::{
    ExtraField,
    IdentityProof,
    IdentityProofType,
    PaymentLink as DbPaymentLink,
    PaymentOption,
};
use mitra_utils::did::Did;

use crate::activitypub::{
    deserialization::deserialize_string_array,
    vocabulary::{
        IDENTITY_PROOF,
        LINK,
        PROPERTY_VALUE,
    },
};
use crate::errors::ValidationError;
use crate::ethereum::identity::verify_eip191_signature;
use crate::identity::{
    claims::create_identity_claim,
    minisign::{
        parse_minisign_signature,
        verify_minisign_signature,
    },
};
use crate::json_signatures::proofs::{
    PROOF_TYPE_ID_EIP191,
    PROOF_TYPE_ID_MINISIGN,
};
use crate::web_client::urls::get_subscription_page_url;

use super::types::ActorAttachment;

pub fn attach_identity_proof(
    proof: IdentityProof,
) -> ActorAttachment {
    let proof_type_str = match proof.proof_type {
        IdentityProofType::LegacyEip191IdentityProof => PROOF_TYPE_ID_EIP191,
        IdentityProofType::LegacyMinisignIdentityProof => PROOF_TYPE_ID_MINISIGN,
    };
    ActorAttachment {
        object_type: IDENTITY_PROOF.to_string(),
        name: proof.issuer.to_string(),
        value: None,
        href: None,
        signature_algorithm: Some(proof_type_str.to_string()),
        signature_value: Some(proof.value),
    }
}

pub fn parse_identity_proof(
    actor_id: &str,
    attachment: &ActorAttachment,
) -> Result<IdentityProof, ValidationError> {
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
            let signature_bin = parse_minisign_signature(signature)
                .map_err(|_| ValidationError("invalid signature encoding"))?;
            verify_minisign_signature(
                did_key,
                &message,
                &signature_bin,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
        },
        Did::Pkh(ref did_pkh) => {
            if !matches!(proof_type, IdentityProofType::LegacyEip191IdentityProof) {
                return Err(ValidationError("incorrect proof type"));
            };
            verify_eip191_signature(
                did_pkh,
                &message,
                signature,
            ).map_err(|_| ValidationError("invalid identity proof"))?;
        },
    };
    let proof = IdentityProof {
        issuer: did,
        proof_type: proof_type,
        value: signature.to_string(),
    };
    Ok(proof)
}

/// https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-0ea0.md
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
    let (name, href) = match payment_option {
        // Local actors can't have payment links
        PaymentOption::Link(_) => unimplemented!(),
        PaymentOption::EthereumSubscription(_) => {
            let name = PAYMENT_LINK_NAME_ETHEREUM.to_string();
            let href = get_subscription_page_url(instance_url, username);
            (name, href)
        },
        PaymentOption::MoneroSubscription(_) => {
            let name = PAYMENT_LINK_NAME_MONERO.to_string();
            let href = get_subscription_page_url(instance_url, username);
            (name, href)
        },
    };
    PaymentLink {
        object_type: LINK.to_string(),
        name: name,
        href: href,
        rel: vec![PAYMENT_LINK_RELATION_TYPE.to_string()],
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
    attachment: &ActorAttachment,
) -> Result<ExtraField, ValidationError> {
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
    use mitra_utils::{
        caip2::ChainId,
    };
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_extra_field() {
        let field = ExtraField {
            name: "test".to_string(),
            value: "value".to_string(),
            value_source: None,
        };
        let attachment = attach_extra_field(field.clone());
        assert_eq!(attachment.object_type, PROPERTY_VALUE);

        let parsed_field = parse_property_value(&attachment).unwrap();
        assert_eq!(parsed_field.name, field.name);
        assert_eq!(parsed_field.value, field.value);
    }

    #[test]
    fn test_payment_option() {
        let username = "testuser";
        let payment_option =
            PaymentOption::ethereum_subscription(ChainId::ethereum_mainnet());
        let subscription_page_url =
            "https://example.com/@testuser/subscription";
        let attachment = attach_payment_option(
            INSTANCE_URL,
            username,
            payment_option,
        );
        assert_eq!(attachment.object_type, LINK);
        assert_eq!(attachment.name, "EthereumSubscription");
        assert_eq!(attachment.href, subscription_page_url);
        assert_eq!(attachment.rel[0], "payment");

        let attachment_value = serde_json::to_value(attachment).unwrap();
        let parsed_option = parse_payment_option(&attachment_value).unwrap();
        let link = match parsed_option {
            PaymentOption::Link(link) => link,
            _ => panic!("wrong option"),
        };
        assert_eq!(link.name, "EthereumSubscription");
        assert_eq!(link.href, subscription_page_url);
    }
}
