use serde::Deserialize;

use mitra_models::{
    profiles::types::PaymentOption,
};
use mitra_utils::caip19::AssetType;

use crate::validators::errors::ValidationError;

use super::constants::{
    ACTION_DELIVER_SERVICE,
    ACTION_TRANSFER,
    CLASS_CONTENT,
    UNIT_ONE,
    UNIT_SECOND,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quantity {
    has_unit: String,
    has_numerical_value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliverServiceIntent {
    action: String,
    resource_conforms_to: String,
    resource_quantity: Quantity,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferIntent {
    action: String,
    resource_conforms_to: String,
    resource_quantity: Quantity,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    id: String,
    publishes: DeliverServiceIntent,
    reciprocal: TransferIntent,
    unit_based: bool,
}

pub fn parse_proposal(
    proposal: Proposal,
) -> Result<PaymentOption, ValidationError> {
    // Primary intent
    if proposal.publishes.action != ACTION_DELIVER_SERVICE {
        return Err(ValidationError("unexpected action"));
    };
    if proposal.publishes.resource_conforms_to != CLASS_CONTENT {
        return Err(ValidationError("unexpected resource type"));
    };
    if proposal.publishes.resource_quantity.has_unit != UNIT_SECOND {
        return Err(ValidationError("unexpected time unit"));
    };
    if proposal.publishes.resource_quantity.has_numerical_value != "1" {
        return Err(ValidationError("unexpected time unit"));
    };
    if !proposal.unit_based {
        return Err(ValidationError("proposal is not unit based"));
    };
    // Reciprocal intent
    if proposal.reciprocal.action != ACTION_TRANSFER {
        return Err(ValidationError("unexpected action"));
    };
    let asset_type = AssetType::from_uri(&proposal.reciprocal.resource_conforms_to)
        .map_err(|_| ValidationError("invalid asset type"))?;
    if !asset_type.is_monero() {
        return Err(ValidationError("unexpected asset type"));
    };
    if proposal.reciprocal.resource_quantity.has_unit != UNIT_ONE {
        return Err(ValidationError("unexpected unit"));
    };
    let price = proposal
        .reciprocal.resource_quantity.has_numerical_value
        .parse::<u64>()
        .map_err(|_| ValidationError("invalid quantity"))?;
    // Create payment option
    let payment_option = PaymentOption::remote_monero_subscription(
        asset_type.chain_id,
        price,
        proposal.id,
    );
    Ok(payment_option)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use mitra_utils::caip2::ChainId;
    use super::*;

    #[test]
    fn test_parse_proposal() {
        let value = json!({
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
        });
        let proposal: Proposal = serde_json::from_value(value).unwrap();
        let payment_option = parse_proposal(proposal).unwrap();
        let payment_info = match payment_option {
            PaymentOption::RemoteMoneroSubscription(info) => info,
            _ => panic!("unexpected option type"),
        };
        assert_eq!(payment_info.chain_id, ChainId::monero_mainnet());
        assert_eq!(payment_info.price, 20000);
        assert_eq!(
            payment_info.object_id,
            "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270",
        );
    }
}
