use serde::Serialize;

use mitra_models::{
    database::DatabaseTypeError,
    invoices::types::DbInvoice,
    profiles::types::MoneroSubscription,
};
use mitra_utils::caip10::AccountId;

use crate::activitypub::{
    constants::PAYMENT_LINK_RELATION_TYPE,
    identifiers::{
        local_actor_proposal_id,
        local_agreement_id,
    },
    valueflows::builders::{
        fep_0837_primary_fragment_id,
        fep_0837_reciprocal_fragment_id,
        Quantity,
    },
    vocabulary::{AGREEMENT, COMMITMENT, LINK},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Commitment {
    #[serde(rename = "type")]
    object_type: String,

    id: String,
    satisfies: String,
    resource_quantity: Quantity,
}

/// https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PaymentLink {
    #[serde(rename = "type")]
    object_type: String,

    href: String,
    rel: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agreement {
    #[serde(rename = "type")]
    object_type: String,

    id: String,
    clauses: (Commitment, Commitment),
    url: PaymentLink,
}

pub fn build_agreement(
    instance_url: &str,
    username: &str,
    payment_info: &MoneroSubscription,
    invoice: &DbInvoice,
) -> Result<Agreement, DatabaseTypeError> {
    let proposal_id = local_actor_proposal_id(
        instance_url,
        username,
        &payment_info.chain_id,
    );
    let agreement_id = local_agreement_id(instance_url, &invoice.id);
    let amount = invoice.amount_u64()?;
    let duration = amount / payment_info.price.get();
    let primary_commitment = Commitment {
        object_type: COMMITMENT.to_string(),
        id: fep_0837_primary_fragment_id(&agreement_id),
        satisfies: fep_0837_primary_fragment_id(&proposal_id),
        resource_quantity: Quantity::duration(duration),
    };
    let reciprocal_commitment = Commitment {
        object_type: COMMITMENT.to_string(),
        id: fep_0837_reciprocal_fragment_id(&agreement_id),
        satisfies: fep_0837_reciprocal_fragment_id(&proposal_id),
        resource_quantity: Quantity::currency_amount(amount),
    };
    let account_id = AccountId {
        chain_id: invoice.chain_id.inner().clone(),
        // Will panic if inoice status is Requested
        address: invoice.try_payment_address()?,
    };
    let payment_link = PaymentLink {
        object_type: LINK.to_string(),
        href: account_id.to_uri(),
        rel: vec![PAYMENT_LINK_RELATION_TYPE.to_string()],
    };
    let agreement = Agreement {
        object_type: AGREEMENT.to_string(),
        id: agreement_id,
        clauses: (primary_commitment, reciprocal_commitment),
        url: payment_link,
    };
    Ok(agreement)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use serde_json::json;
    use mitra_models::invoices::types::DbChainId;
    use mitra_utils::caip2::ChainId;
    use super::*;

    #[test]
    fn test_build_agreement() {
        let instance_url = "https://test.example";
        let username = "alice";
        let chain_id = ChainId::monero_mainnet();
        let payment_info = MoneroSubscription {
            chain_id: chain_id.clone(),
            price: NonZeroU64::new(20000).unwrap(),
            payout_address: "test".to_string(),
        };
        let invoice_id = "edc374aa-e580-4a58-9404-f3e8bf8556b2".parse().unwrap();
        let invoice = DbInvoice {
            id: invoice_id,
            chain_id: DbChainId::new(&chain_id),
            payment_address: Some("8xyz".to_string()),
            amount: 60000000,
            ..Default::default()
        };
        let proposal = build_agreement(
            instance_url,
            username,
            &payment_info,
            &invoice,
        ).unwrap();

        let expected_value = json!({
            "type": "Agreement",
            "id": "https://test.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2",
            "clauses": [
                {
                    "id": "https://test.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2#primary",
                    "type": "Commitment",
                    "satisfies": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270#primary",
                    "resourceQuantity": {
                        "hasUnit": "second",
                        "hasNumericalValue": "3000",
                    },
                },
                {
                    "id": "https://test.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2#reciprocal",
                    "type": "Commitment",
                    "satisfies": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270#reciprocal",
                    "resourceQuantity": {
                        "hasUnit": "one",
                        "hasNumericalValue": "60000000",
                    },
                },
            ],
            "url": {
                "type": "Link",
                "href": "caip:10:monero:418015bb9ae982a1975da7d79277c270:8xyz",
                "rel": ["payment"],
            },
        });
        assert_eq!(
            serde_json::to_value(proposal).unwrap(),
            expected_value,
        );
    }
}
