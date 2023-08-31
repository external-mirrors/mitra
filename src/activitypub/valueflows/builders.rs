/// https://codeberg.org/silverpill/feps/src/branch/main/0837/fep-0837.md
use serde::Serialize;

use mitra_models::{
    database::DatabaseTypeError,
    invoices::types::DbInvoice,
    profiles::types::MoneroSubscription,
};
use mitra_utils::{
    caip10::AccountId,
    caip19::AssetType,
};

use crate::activitypub::{
    constants::{
        AP_PUBLIC,
        PAYMENT_LINK_RELATION_TYPE,
        UNITS_OF_MEASURE_CONTEXT,
        W3ID_VALUEFLOWS_CONTEXT,
    },
    identifiers::{
        local_actor_id,
        local_actor_proposal_id,
        local_agreement_id,
    },
    types::{build_default_context, Context},
    vocabulary::{
        AGREEMENT,
        COMMITMENT,
        INTENT,
        LINK,
        PROPOSAL,
    },
};

use super::constants::{
    ACTION_DELIVER_SERVICE,
    ACTION_TRANSFER,
    CLASS_CONTENT,
    UNIT_ONE,
    UNIT_SECOND,
};

fn build_valueflows_context() -> Context {
    let mut context = build_default_context();
    let vf_map = [
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
        ("Agreement", "vf:Agreement"),
        ("commitments", "vf:commitments"),
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Quantity {
    has_unit: String,
    has_numerical_value: String,
}

impl Quantity {
    fn currency_amount(value: u64) -> Self {
        Self {
            has_unit: UNIT_ONE.to_string(),
            has_numerical_value: value.to_string(),
        }
    }

    fn duration(value: u64) -> Self {
        Self {
            has_unit: UNIT_SECOND.to_string(),
            has_numerical_value: value.to_string(),
        }
    }
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
    _context: Context,

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
        _context: build_valueflows_context(),
        object_type: PROPOSAL.to_string(),
        id: proposal_id.clone(),
        attributed_to: actor_id.clone(),
        name: proposal_name.to_string(),
        publishes: DeliverServiceIntent {
            object_type: INTENT.to_string(),
            id: fep_0837_primary_fragment_id(&proposal_id),
            action: ACTION_DELIVER_SERVICE.to_string(),
            resource_conforms_to: CLASS_CONTENT.to_string(),
            resource_quantity: Quantity::duration(1),
            provider: actor_id.clone(),
        },
        reciprocal: TransferIntent {
            object_type: INTENT.to_string(),
            id: fep_0837_reciprocal_fragment_id(&proposal_id),
            action: ACTION_TRANSFER.to_string(),
            resource_conforms_to: asset_type.to_uri(),
            resource_quantity:
                // piconeros per second
                Quantity::currency_amount(payment_info.price.get()),
            receiver: actor_id,
        },
        unit_based: true,
        to: AP_PUBLIC.to_string(),
    }
}

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
struct Agreement {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    object_type: String,

    id: String,
    commitments: (Commitment, Commitment),
    url: PaymentLink,
}

#[allow(dead_code)]
fn build_agreement(
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
        address: invoice.payment_address.clone(),
    };
    let payment_link = PaymentLink {
        object_type: LINK.to_string(),
        href: account_id.to_uri(),
        rel: vec![PAYMENT_LINK_RELATION_TYPE.to_string()],
    };
    let agreement = Agreement {
        _context: build_valueflows_context(),
        object_type: AGREEMENT.to_string(),
        id: agreement_id,
        commitments: (primary_commitment, reciprocal_commitment),
        url: payment_link,
    };
    Ok(agreement)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use serde_json::json;
    use mitra_models::invoices::types::{DbChainId, InvoiceStatus};
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
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "proofValue": "sec:proofValue",
                    "proofPurpose": "sec:proofPurpose",
                    "verificationMethod": "sec:verificationMethod",
                    "mitra": "http://jsonld.mitra.social#",
                    "MitraJcsRsaSignature2022": "mitra:MitraJcsRsaSignature2022",
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
                    "Agreement": "vf:Agreement",
                    "commitments": "vf:commitments",
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
            sender_id: Default::default(),
            recipient_id: Default::default(),
            chain_id: DbChainId::new(&chain_id),
            payment_address: "8xyz".to_string(),
            amount: 60000000,
            invoice_status: InvoiceStatus::Open,
            payout_tx_id: None,
            created_at: Default::default(),
            updated_at: Default::default(),
        };
        let proposal = build_agreement(
            instance_url,
            username,
            &payment_info,
            &invoice,
        ).unwrap();

        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "proofValue": "sec:proofValue",
                    "proofPurpose": "sec:proofPurpose",
                    "verificationMethod": "sec:verificationMethod",
                    "mitra": "http://jsonld.mitra.social#",
                    "MitraJcsRsaSignature2022": "mitra:MitraJcsRsaSignature2022",
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
                    "Agreement": "vf:Agreement",
                    "commitments": "vf:commitments",
                    "Commitment": "vf:Commitment",
                    "satisfies": "vf:satisfies",
                    "resourceConformsTo": "vf:resourceConformsTo",
                    "resourceQuantity": "vf:resourceQuantity",
                    "hasUnit": "om2:hasUnit",
                    "hasNumericalValue": "om2:hasNumericalValue",
                },
            ],
            "type": "Agreement",
            "id": "https://test.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2",
            "commitments": [
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
