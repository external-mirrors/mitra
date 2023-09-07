use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    profiles::types::{
        DbActor,
        RemoteMoneroSubscription,
    },
    users::types::User,
};

use crate::activitypub::{
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    types::Context,
    valueflows::{
        builders::{
            build_valueflows_context,
            fep_0837_primary_fragment_id,
            fep_0837_reciprocal_fragment_id,
            Quantity,
        },
    },
    vocabulary::{AGREEMENT, COMMITMENT, OFFER},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Commitment {
    #[serde(rename = "type")]
    object_type: String,

    satisfies: String,
    resource_quantity: Quantity,
}

// Agreement draft
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Agreement {
    #[serde(rename = "type")]
    object_type: String,

    clauses: (Commitment, Commitment),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OfferAgreement {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Agreement,
    to: String,
}

fn build_offer_agreement(
    instance_url: &str,
    sender_username: &str,
    proposer_actor_id: &str,
    subscription_option: &RemoteMoneroSubscription,
    invoice_id: &Uuid,
    invoice_amount: u64,
) -> OfferAgreement {
    let proposal_id = subscription_option.object_id.clone();
    let primary_intent_id = fep_0837_primary_fragment_id(&proposal_id);
    let reciprocal_intent_id = fep_0837_reciprocal_fragment_id(&proposal_id);
    let duration = invoice_amount / subscription_option.price;
    let actor_id = local_actor_id(instance_url, sender_username);
    let activity_id = local_object_id(instance_url, invoice_id);
    let agreement = Agreement {
        object_type: AGREEMENT.to_string(),
        clauses: (
            Commitment {
                object_type: COMMITMENT.to_string(),
                satisfies: primary_intent_id,
                resource_quantity: Quantity::duration(duration),
            },
            Commitment {
                object_type: COMMITMENT.to_string(),
                satisfies: reciprocal_intent_id,
                resource_quantity: Quantity::currency_amount(invoice_amount),
            },
        ),
    };
    let activity = OfferAgreement {
        _context: build_valueflows_context(),
        activity_type: OFFER.to_string(),
        id: activity_id,
        actor: actor_id,
        object: agreement,
        to: proposer_actor_id.to_string(),
    };
    activity
}

pub fn prepare_offer_agreement(
    instance: &Instance,
    sender: &User,
    proposer_actor: &DbActor,
    subscription_option: &RemoteMoneroSubscription,
    invoice_id: &Uuid,
    invoice_amount: u64,
) -> OutgoingActivity {
    let activity = build_offer_agreement(
        &instance.url(),
        &sender.profile.username,
        &proposer_actor.id,
        subscription_option,
        invoice_id,
        invoice_amount,
    );
    let recipients = vec![proposer_actor.clone()];
    OutgoingActivity::new(
        instance,
        sender,
        activity,
        recipients,
    )
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use serde_json::json;
    use mitra_utils::caip2::ChainId;
    use super::*;

    #[test]
    fn test_build_offer_agreement() {
        let instance_url = "https://local.example";
        let sender_username = "payer";
        let proposal_id = "https://remote.example/proposals/1";
        let proposer_actor_id = "https://remote.example/users/test";
        let subscription_option = RemoteMoneroSubscription {
            chain_id: ChainId::monero_mainnet(),
            price: NonZeroU64::new(20000).unwrap(),
            object_id: proposal_id.to_string(),
            fep_0837_enabled: true,
        };
        let invoice_id =
            "46d160ae-af12-484d-9f44-419f00fc1b31".parse().unwrap();
        let invoice_amount = 200000;
        let activity = build_offer_agreement(
            instance_url,
            sender_username,
            proposer_actor_id,
            &subscription_option,
            &invoice_id,
            invoice_amount,
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
                    "clauses": "vf:clauses",
                    "Commitment": "vf:Commitment",
                    "satisfies": "vf:satisfies",
                    "resourceConformsTo": "vf:resourceConformsTo",
                    "resourceQuantity": "vf:resourceQuantity",
                    "hasUnit": "om2:hasUnit",
                    "hasNumericalValue": "om2:hasNumericalValue",
                },
            ],
            "type": "Offer",
            "id": "https://local.example/objects/46d160ae-af12-484d-9f44-419f00fc1b31",
            "actor": "https://local.example/users/payer",
            "object": {
                "type": "Agreement",
                "clauses": [
                    {
                        "type": "Commitment",
                        "satisfies": "https://remote.example/proposals/1#primary",
                        "resourceQuantity": {
                            "hasUnit": "second",
                            "hasNumericalValue": "10",
                        },
                    },
                    {
                        "type": "Commitment",
                        "satisfies": "https://remote.example/proposals/1#reciprocal",
                        "resourceQuantity": {
                            "hasUnit": "one",
                            "hasNumericalValue": "200000",
                        },
                    },
                ],
            },
            "to": "https://remote.example/users/test",
        });
        assert_eq!(
            serde_json::to_value(activity).unwrap(),
            expected_value,
        );
    }
}
