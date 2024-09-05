/// https://codeberg.org/silverpill/feps/src/branch/main/0837/fep-0837.md
use serde::Serialize;

use apx_core::caip19::AssetType;
use mitra_federation::constants::AP_PUBLIC;
use mitra_models::profiles::types::MoneroSubscription;

use crate::{
    contexts::{
        build_default_context,
        Context,
        UNITS_OF_MEASURE_CONTEXT,
        W3ID_VALUEFLOWS_CONTEXT,
    },
    identifiers::{
        local_actor_id,
        local_actor_proposal_id,
    },
    vocabulary::{INTENT, PROPOSAL},
};

pub const PURPOSE_OFFER: &str = "offer";

// https://www.valueflo.ws/concepts/actions/#action-definitions
pub const ACTION_DELIVER_SERVICE: &str = "deliverService";
pub const ACTION_TRANSFER: &str = "transfer";

pub const CLASS_USER_GENERATED_CONTENT: &str = "https://www.wikidata.org/wiki/Q579716";

// http://www.ontology-of-units-of-measure.org/resource/om-2/one
pub const UNIT_ONE: &str = "one";
// http://www.ontology-of-units-of-measure.org/resource/om-2/second-Time
pub const UNIT_SECOND: &str = "second";

pub fn build_valueflows_context() -> Context {
    let mut context = build_default_context();
    let vf_map = [
        // https://www.valueflo.ws/specification/all_vf.html
        ("vf", W3ID_VALUEFLOWS_CONTEXT),
        ("om2", UNITS_OF_MEASURE_CONTEXT),
        ("Proposal", "vf:Proposal"),
        ("Intent", "vf:Intent"),
        ("purpose", "vf:purpose"),
        ("publishes", "vf:publishes"),
        ("reciprocal", "vf:reciprocal"),
        ("unitBased", "vf:unitBased"),
        ("action", "vf:action"),
        ("Agreement", "vf:Agreement"),
        ("stipulates", "vf:stipulates"),
        ("stipulatesReciprocal", "vf:stipulatesReciprocal"),
        ("Commitment", "vf:Commitment"),
        ("satisfies", "vf:satisfies"),
        ("resourceConformsTo", "vf:resourceConformsTo"),
        ("resourceQuantity", "vf:resourceQuantity"),
        ("hasUnit", "om2:hasUnit"),
        ("hasNumericalValue", "om2:hasNumericalValue"),
    ];
    context.3.extend(vf_map);
    context
}

pub fn fep_0837_primary_fragment_id(url: &str) -> String {
    format!("{}#primary", url)
}

pub fn fep_0837_reciprocal_fragment_id(url: &str) -> String {
    format!("{}#reciprocal", url)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quantity {
    has_unit: String,
    has_numerical_value: String,
}

impl Quantity {
    pub fn currency_amount(value: u64) -> Self {
        Self {
            has_unit: UNIT_ONE.to_string(),
            has_numerical_value: value.to_string(),
        }
    }

    pub fn duration(value: u64) -> Self {
        Self {
            has_unit: UNIT_SECOND.to_string(),
            has_numerical_value: value.to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Intent {
    #[serde(rename = "type")]
    object_type: String,
    id: String,
    action: String,
    resource_conforms_to: String,
    resource_quantity: Quantity,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    object_type: String,
    id: String,
    purpose: String,
    attributed_to: String,
    name: String,
    publishes: Intent,
    reciprocal: Intent,
    unit_based: bool,
    to: String,
}

// https://www.valueflo.ws/concepts/proposals/
pub fn build_proposal(
    instance_url: &str,
    username: &str,
    payment_info: &MoneroSubscription,
) -> Proposal {
    let actor_id = local_actor_id(
        instance_url,
        username,
    );
    let proposal_id = local_actor_proposal_id(
        &actor_id,
        &payment_info.chain_id,
    );
    let proposal_name = "Pay for subscription";
    let asset_type = AssetType::monero(&payment_info.chain_id)
        .expect("chain should belong to monero namespace");
    Proposal {
        _context: build_valueflows_context(),
        object_type: PROPOSAL.to_string(),
        id: proposal_id.clone(),
        purpose: PURPOSE_OFFER.to_string(),
        attributed_to: actor_id.clone(),
        name: proposal_name.to_string(),
        publishes: Intent {
            object_type: INTENT.to_string(),
            id: fep_0837_primary_fragment_id(&proposal_id),
            action: ACTION_DELIVER_SERVICE.to_string(),
            resource_conforms_to: CLASS_USER_GENERATED_CONTENT.to_string(),
            resource_quantity: Quantity::duration(1),
        },
        reciprocal: Intent {
            object_type: INTENT.to_string(),
            id: fep_0837_reciprocal_fragment_id(&proposal_id),
            action: ACTION_TRANSFER.to_string(),
            resource_conforms_to: asset_type.to_uri(),
            resource_quantity:
                // piconeros per second
                Quantity::currency_amount(payment_info.price.get()),
        },
        unit_based: true,
        to: AP_PUBLIC.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use serde_json::json;
    use apx_core::caip2::ChainId;
    use super::*;

    #[test]
    fn test_build_proposal() {
        let instance_url = "https://test.example";
        let username = "alice";
        let payment_info = MoneroSubscription {
            chain_id: ChainId::monero_mainnet(),
            price: NonZeroU64::new(20000).unwrap(),
            payout_address: "test".to_string(),
        };
        let proposal = build_proposal(
            instance_url,
            username,
            &payment_info,
        );

        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji",
                    "litepub": "http://litepub.social/ns#",
                    "EmojiReact": "litepub:EmojiReact",
                    "vf": "https://w3id.org/valueflows/ont/vf#",
                    "om2": "http://www.ontology-of-units-of-measure.org/resource/om-2/",
                    "Proposal": "vf:Proposal",
                    "Intent": "vf:Intent",
                    "purpose": "vf:purpose",
                    "publishes": "vf:publishes",
                    "reciprocal": "vf:reciprocal",
                    "unitBased": "vf:unitBased",
                    "action": "vf:action",
                    "Agreement": "vf:Agreement",
                    "stipulates": "vf:stipulates",
                    "stipulatesReciprocal": "vf:stipulatesReciprocal",
                    "Commitment": "vf:Commitment",
                    "satisfies": "vf:satisfies",
                    "resourceConformsTo": "vf:resourceConformsTo",
                    "resourceQuantity": "vf:resourceQuantity",
                    "hasUnit": "om2:hasUnit",
                    "hasNumericalValue": "om2:hasNumericalValue",
                },
            ],
            "type": "Proposal",
            "id": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270",
            "purpose": "offer",
            "attributedTo": "https://test.example/users/alice",
            "name": "Pay for subscription",
            "publishes": {
                "type": "Intent",
                "id": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270#primary",
                "action": "deliverService",
                "resourceConformsTo": "https://www.wikidata.org/wiki/Q579716",
                "resourceQuantity": {
                    "hasUnit": "second",
                    "hasNumericalValue": "1",
                },
            },
            "reciprocal": {
                "type": "Intent",
                "id": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270#reciprocal",
                "action": "transfer",
                "resourceConformsTo": "caip:19:monero:418015bb9ae982a1975da7d79277c270/slip44:128",
                "resourceQuantity": {
                    "hasUnit": "one",
                    "hasNumericalValue": "20000",
                },
            },
            "unitBased": true,
            "to": "https://www.w3.org/ns/activitystreams#Public",
        });
        assert_eq!(
            serde_json::to_value(proposal).unwrap(),
            expected_value,
        );
    }
}
