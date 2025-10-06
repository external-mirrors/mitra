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

use crate::{
    contexts::Context,
    deliverer::Recipient,
    identifiers::{local_activity_id, local_actor_id},
    queues::OutgoingActivityJobData,
    vocabulary::{AGREEMENT, COMMITMENT, OFFER},
};

use super::agreement::{Commitment, Agreement};
use super::proposal::{
    build_valueflows_context,
    fep_0837_primary_fragment_id,
    fep_0837_reciprocal_fragment_id,
    Quantity,
};

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
    instance_uri: &str,
    sender_username: &str,
    proposer_actor_id: &str,
    subscription_option: &RemoteMoneroSubscription,
    invoice_id: Uuid,
    invoice_amount: u64,
) -> OfferAgreement {
    let proposal_id = subscription_option.object_id.clone();
    let primary_intent_id = fep_0837_primary_fragment_id(&proposal_id);
    let reciprocal_intent_id = fep_0837_reciprocal_fragment_id(&proposal_id);
    let duration = invoice_amount / subscription_option.price;
    let actor_id = local_actor_id(instance_uri, sender_username);
    let activity_id = local_activity_id(instance_uri, OFFER, invoice_id);
    let primary_commitment = Commitment {
        id: None,
        object_type: COMMITMENT.to_string(),
        satisfies: primary_intent_id,
        resource_quantity: Quantity::duration(duration),
    };
    let reciprocal_commitment = Commitment {
        id: None,
        object_type: COMMITMENT.to_string(),
        satisfies: reciprocal_intent_id,
        resource_quantity: Quantity::currency_amount(invoice_amount),
    };
    let agreement = Agreement {
        id: None,
        object_type: AGREEMENT.to_string(),
        attributed_to: None,
        stipulates: primary_commitment,
        stipulates_reciprocal: reciprocal_commitment,
        url: None, // pre-agreement shouldn't have payment link
        preview: None, // pre-agreement doesn't have status
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
    invoice_id: Uuid,
    invoice_amount: u64,
) -> OutgoingActivityJobData {
    let activity = build_offer_agreement(
        instance.uri_str(),
        &sender.profile.username,
        &proposer_actor.id,
        subscription_option,
        invoice_id,
        invoice_amount,
    );
    let recipients = Recipient::for_inbox(proposer_actor);
    OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    )
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use apx_core::caip2::ChainId;
    use serde_json::json;
    use uuid::uuid;
    use super::*;

    #[test]
    fn test_build_offer_agreement() {
        let instance_uri = "https://local.example";
        let sender_username = "payer";
        let proposal_id = "https://remote.example/proposals/1";
        let proposer_actor_id = "https://remote.example/users/test";
        let subscription_option = RemoteMoneroSubscription {
            chain_id: ChainId::monero_mainnet(),
            price: NonZeroU64::new(20000).unwrap(),
            amount_min: Some(1_000_000_000),
            object_id: proposal_id.to_string(),
            fep_0837_enabled: true,
        };
        let invoice_id = uuid!("46d160ae-af12-484d-9f44-419f00fc1b31");
        let invoice_amount = 200000;
        let activity = build_offer_agreement(
            instance_uri,
            sender_username,
            proposer_actor_id,
            &subscription_option,
            invoice_id,
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
                    "minimumQuantity": "vf:minimumQuantity",
                    "hasUnit": "om2:hasUnit",
                    "hasNumericalValue": "om2:hasNumericalValue",
                },
            ],
            "type": "Offer",
            "id": "https://local.example/activities/offer/46d160ae-af12-484d-9f44-419f00fc1b31",
            "actor": "https://local.example/users/payer",
            "object": {
                "type": "Agreement",
                "stipulates": {
                    "type": "Commitment",
                    "satisfies": "https://remote.example/proposals/1#primary",
                    "resourceQuantity": {
                        "hasUnit": "second",
                        "hasNumericalValue": "10",
                    },
                },
                "stipulatesReciprocal": {
                    "type": "Commitment",
                    "satisfies": "https://remote.example/proposals/1#reciprocal",
                    "resourceQuantity": {
                        "hasUnit": "one",
                        "hasNumericalValue": "200000",
                    },
                },
            },
            "to": "https://remote.example/users/test",
        });
        assert_eq!(
            serde_json::to_value(activity).unwrap(),
            expected_value,
        );
    }
}
