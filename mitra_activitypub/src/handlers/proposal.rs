use std::str::FromStr;

use apx_core::caip19::AssetType;
use serde::Deserialize;

use mitra_models::{
    profiles::types::PaymentOption,
};
use mitra_validators::errors::ValidationError;

use crate::{
    builders::proposal::{
        ACTION_DELIVER_SERVICE,
        ACTION_TRANSFER,
        CLASS_USER_GENERATED_CONTENT,
        PURPOSE_OFFER,
        UNIT_ONE,
        UNIT_SECOND,
    },
    identifiers::canonicalize_id,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quantity {
    has_unit: String,
    has_numerical_value: String,
}

impl Quantity {
    pub fn parse_currency_amount<T: FromStr>(&self) -> Result<T, ValidationError> {
        if self.has_unit != UNIT_ONE {
            return Err(ValidationError("unexpected unit"));
        };
        let amount = self.has_numerical_value
            .parse()
            .map_err(|_| ValidationError("invalid quantity"))?;
        Ok(amount)
    }

    pub fn parse_duration(&self) -> Result<u64, ValidationError> {
        if self.has_unit != UNIT_SECOND {
            return Err(ValidationError("unexpected time unit"));
        };
        let duration = self.has_numerical_value
            .parse()
            .map_err(|_| ValidationError("invalid quantity"))?;
        Ok(duration)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    action: String,
    resource_conforms_to: String,
    resource_quantity: Quantity,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: String,
    purpose: String,

    publishes: Intent,
    reciprocal: Intent,

    unit_based: bool,
}

pub fn parse_proposal(
    proposal: Proposal,
) -> Result<PaymentOption, ValidationError> {
    let canonical_proposal_id = canonicalize_id(&proposal.id)?;
    // Purpose
    if proposal.purpose != PURPOSE_OFFER {
        return Err(ValidationError("proposal is not an offer"));
    };
    // Primary intent
    if proposal.publishes.action != ACTION_DELIVER_SERVICE {
        return Err(ValidationError("unexpected action"));
    };
    if proposal.publishes.resource_conforms_to.as_str() != CLASS_USER_GENERATED_CONTENT {
        return Err(ValidationError("unexpected resource type"));
    };
    let duration = proposal.publishes.resource_quantity
        .parse_duration()?;
    if duration != 1 {
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
    let price = proposal.reciprocal.resource_quantity
        .parse_currency_amount()?;
    // Create payment option
    let payment_option = PaymentOption::remote_monero_subscription(
        asset_type.chain_id,
        price,
        canonical_proposal_id.to_string(),
        true,
    );
    Ok(payment_option)
}

#[cfg(test)]
mod tests {
    use apx_core::caip2::ChainId;
    use serde_json::json;
    use super::*;

    #[test]
    fn test_parse_proposal() {
        let value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji",
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
        });
        let proposal: Proposal = serde_json::from_value(value).unwrap();
        let payment_option = parse_proposal(proposal).unwrap();
        let payment_info = match payment_option {
            PaymentOption::RemoteMoneroSubscription(info) => info,
            _ => panic!("unexpected option type"),
        };
        assert_eq!(payment_info.chain_id, ChainId::monero_mainnet());
        assert_eq!(payment_info.price.get(), 20000);
        assert_eq!(
            payment_info.object_id,
            "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270",
        );
    }
}
