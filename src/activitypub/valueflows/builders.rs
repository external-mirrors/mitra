/// https://codeberg.org/silverpill/feps/src/branch/main/0837/fep-0837.md
use std::collections::HashMap;

use serde::Serialize;

use mitra_models::profiles::types::MoneroSubscription;
use mitra_utils::caip19::AssetType;

use crate::activitypub::{
    constants::{
        AP_CONTEXT,
        AP_PUBLIC,
        UNITS_OF_MEASURE_CONTEXT,
        W3ID_VALUEFLOWS_CONTEXT,
    },
    identifiers::{
        local_actor_id,
        local_actor_proposal_id,
    },
    vocabulary::{INTENT, PROPOSAL},
};

use super::constants::{
    ACTION_DELIVER_SERVICE,
    ACTION_TRANSFER,
    CLASS_CONTENT,
    UNIT_ONE,
    UNIT_SECOND,
};

type Context = (
    &'static str,
    HashMap<&'static str, &'static str>,
);

fn build_proposal_context() -> Context {
    (
        AP_CONTEXT,
        HashMap::from([
            // https://www.valueflo.ws/specification/all_vf.html
            ("vf", W3ID_VALUEFLOWS_CONTEXT),
            ("om2", UNITS_OF_MEASURE_CONTEXT),
            ("Proposal", "vf:Proposal"),
            ("Intent", "vf:Intent"),
            ("publishes", "vf:publishes"),
            ("reciprocal", "vf:reciprocal"),
            ("unitBased", "vf:unitBased"),
            ("provider", "vf:provider"),
            ("receiver", "vf:receiver"),
            ("action", "vf:action"),
            ("resourceConformsTo", "vf:resourceConformsTo"),
            ("resourceQuantity", "vf:resourceQuantity"),
            ("hasUnit", "om2:hasUnit"),
            ("hasNumericalValue", "om2:hasNumericalValue"),
        ]),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Quantity {
    has_unit: String,
    has_numerical_value: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeliverServiceIntent {
    #[serde(rename = "type")]
    object_type: String,
    id: String,
    action: String,
    resource_conforms_to: String,
    resource_quantity: Quantity,
    provider: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferIntent {
    #[serde(rename = "type")]
    object_type: String,
    id: String,
    action: String,
    resource_conforms_to: String,
    resource_quantity: Quantity,
    receiver: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    object_type: String,
    id: String,
    attributed_to: String,
    name: String,
    publishes: DeliverServiceIntent,
    reciprocal: TransferIntent,
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
        instance_url,
        username,
        &payment_info.chain_id,
    );
    let proposal_name = "Pay for subscription";
    let asset_type = AssetType::monero(&payment_info.chain_id)
        .expect("chain should belong to monero namespace");
    Proposal {
        context: build_proposal_context(),
        object_type: PROPOSAL.to_string(),
        id: proposal_id.clone(),
        attributed_to: actor_id.clone(),
        name: proposal_name.to_string(),
        publishes: DeliverServiceIntent {
            object_type: INTENT.to_string(),
            id: format!("{}#primary", proposal_id),
            action: ACTION_DELIVER_SERVICE.to_string(),
            resource_conforms_to: CLASS_CONTENT.to_string(),
            resource_quantity: Quantity {
                has_unit: UNIT_SECOND.to_string(),
                has_numerical_value: "1".to_string(),
            },
            provider: actor_id.clone(),
        },
        reciprocal: TransferIntent {
            object_type: INTENT.to_string(),
            id: format!("{}#reciprocal", proposal_id),
            action: ACTION_TRANSFER.to_string(),
            resource_conforms_to: asset_type.to_uri(),
            resource_quantity: Quantity {
                has_unit: UNIT_ONE.to_string(),
                // piconeros per second
                has_numerical_value: payment_info.price.to_string(),
            },
            receiver: actor_id,
        },
        unit_based: true,
        to: AP_PUBLIC.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use serde_json::json;
    use mitra_utils::caip2::ChainId;
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
                {
                    "vf": "https://w3id.org/valueflows/",
                    "om2": "http://www.ontology-of-units-of-measure.org/resource/om-2/",
                    "Proposal": "vf:Proposal",
                    "Intent": "vf:Intent",
                    "publishes": "vf:publishes",
                    "reciprocal": "vf:reciprocal",
                    "unitBased": "vf:unitBased",
                    "provider": "vf:provider",
                    "receiver": "vf:receiver",
                    "action": "vf:action",
                    "resourceConformsTo": "vf:resourceConformsTo",
                    "resourceQuantity": "vf:resourceQuantity",
                    "hasUnit": "om2:hasUnit",
                    "hasNumericalValue": "om2:hasNumericalValue",
                },
            ],
            "type": "Proposal",
            "id": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270",
            "attributedTo": "https://test.example/users/alice",
            "name": "Pay for subscription",
            "publishes": {
                "type": "Intent",
                "id": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270#primary",
                "action": "deliverService",
                "resourceConformsTo": "https://www.wikidata.org/wiki/Q1260632",
                "resourceQuantity": {
                    "hasUnit": "second",
                    "hasNumericalValue": "1",
                },
                "provider": "https://test.example/users/alice",
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
                "receiver": "https://test.example/users/alice",
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
