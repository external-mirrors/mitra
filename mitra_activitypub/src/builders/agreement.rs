use apx_core::caip10::AccountId;
use serde::Serialize;

use mitra_models::{
    database::DatabaseTypeError,
    invoices::types::{Invoice, InvoiceStatus},
    profiles::types::MoneroSubscription,
};

use crate::{
    constants::PAYMENT_LINK_RELATION_TYPE,
    identifiers::{
        local_actor_id,
        local_actor_proposal_id,
        local_agreement_id,
    },
    vocabulary::{AGREEMENT, COMMITMENT, LINK, NOTE},
};

use super::proposal::{
    fep_0837_primary_fragment_id,
    fep_0837_reciprocal_fragment_id,
    Quantity,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commitment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "type")]
    pub object_type: String,

    pub satisfies: String,
    pub resource_quantity: Quantity,
}

// https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentLink {
    #[serde(rename = "type")]
    object_type: String,

    href: String,
    rel: Vec<String>,
}

#[derive(Serialize)]
pub struct PaymentStatus {
    #[serde(rename = "type")]
    object_type: String,
    pub name: String,
}

impl From<InvoiceStatus> for PaymentStatus {
    fn from(status: InvoiceStatus) -> Self {
        Self {
            object_type: NOTE.to_owned(),
            name: format!("{status:?}"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agreement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "type")]
    pub object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributed_to: Option<String>,

    pub stipulates: Commitment,
    pub stipulates_reciprocal: Commitment,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<PaymentLink>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PaymentStatus>,
}

/// Builds Agreement object from invoice
pub fn build_agreement(
    instance_uri: &str,
    username: &str,
    payment_info: &MoneroSubscription,
    invoice: &Invoice,
) -> Result<Agreement, DatabaseTypeError> {
    let actor_id = local_actor_id(instance_uri, username);
    let proposal_id = local_actor_proposal_id(
        &actor_id,
        &payment_info.chain_id,
    );
    let agreement_id = local_agreement_id(instance_uri, invoice.id);
    let amount = invoice.amount_u64()?;
    let duration = amount / payment_info.price.get();
    let primary_commitment = Commitment {
        id: Some(fep_0837_primary_fragment_id(&agreement_id)),
        object_type: COMMITMENT.to_string(),
        satisfies: fep_0837_primary_fragment_id(&proposal_id),
        resource_quantity: Quantity::duration(duration),
    };
    let reciprocal_commitment = Commitment {
        id: Some(fep_0837_reciprocal_fragment_id(&agreement_id)),
        object_type: COMMITMENT.to_string(),
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
    let payment_status = PaymentStatus::from(invoice.invoice_status);
    let agreement = Agreement {
        id: Some(agreement_id),
        object_type: AGREEMENT.to_string(),
        attributed_to: Some(actor_id),
        stipulates: primary_commitment,
        stipulates_reciprocal: reciprocal_commitment,
        url: Some(payment_link),
        preview: Some(payment_status),
    };
    Ok(agreement)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use apx_core::caip2::ChainId;
    use serde_json::json;
    use mitra_models::invoices::types::DbChainId;
    use super::*;

    #[test]
    fn test_build_agreement() {
        let instance_uri = "https://test.example";
        let username = "alice";
        let chain_id = ChainId::monero_mainnet();
        let payment_info = MoneroSubscription {
            chain_id: chain_id.clone(),
            price: NonZeroU64::new(20000).unwrap(),
        };
        let invoice_id = "edc374aa-e580-4a58-9404-f3e8bf8556b2".parse().unwrap();
        let invoice = Invoice {
            id: invoice_id,
            chain_id: DbChainId::new(&chain_id),
            payment_address: Some("8xyz".to_string()),
            amount: 60000000,
            ..Default::default()
        };
        let proposal = build_agreement(
            instance_uri,
            username,
            &payment_info,
            &invoice,
        ).unwrap();

        let expected_value = json!({
            "type": "Agreement",
            "id": "https://test.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2",
            "attributedTo": "https://test.example/users/alice",
            "stipulates": {
                "id": "https://test.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2#primary",
                "type": "Commitment",
                "satisfies": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270#primary",
                "resourceQuantity": {
                    "hasUnit": "second",
                    "hasNumericalValue": "3000",
                },
            },
            "stipulatesReciprocal": {
                "id": "https://test.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2#reciprocal",
                "type": "Commitment",
                "satisfies": "https://test.example/users/alice/proposals/monero:418015bb9ae982a1975da7d79277c270#reciprocal",
                "resourceQuantity": {
                    "hasUnit": "one",
                    "hasNumericalValue": "60000000",
                },
            },
            "url": {
                "type": "Link",
                "href": "caip:10:monero:418015bb9ae982a1975da7d79277c270:8xyz",
                "rel": ["payment"],
            },
            "preview": {
                "type": "Note",
                "name": "Open",
            },
        });
        assert_eq!(
            serde_json::to_value(proposal).unwrap(),
            expected_value,
        );
    }
}
