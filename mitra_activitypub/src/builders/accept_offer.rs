use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseError, DatabaseTypeError},
    invoices::types::Invoice,
    profiles::types::{DbActor, MoneroSubscription},
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::{
    contexts::Context,
    deliverer::Recipient,
    identifiers::{local_activity_id, local_actor_id},
    queues::OutgoingActivityJobData,
    vocabulary::ACCEPT,
};

use super::agreement::{build_agreement, Agreement};
use super::proposal::build_valueflows_context;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptOffer {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,
    result: Agreement,
    to: String,
}

fn build_accept_offer(
    instance_uri: &str,
    sender_username: &str,
    subscription_option: &MoneroSubscription,
    invoice: &Invoice,
    remote_actor_id: &str,
    offer_activity_id: &str,
) -> Result<AcceptOffer, DatabaseTypeError> {
    let agreement = build_agreement(
        instance_uri,
        sender_username,
        subscription_option,
        invoice,
    )?;
    let actor_id = local_actor_id(instance_uri, sender_username);
    let activity_id = local_activity_id(instance_uri, ACCEPT, generate_ulid());
    let activity = AcceptOffer {
        _context: build_valueflows_context(),
        activity_type: ACCEPT.to_string(),
        id: activity_id,
        actor: actor_id,
        object: offer_activity_id.to_string(),
        result: agreement,
        to: remote_actor_id.to_string(),
    };
    Ok(activity)
}

pub fn prepare_accept_offer(
    instance: &Instance,
    sender: &User,
    subscription_option: &MoneroSubscription,
    invoice: &Invoice,
    remote_actor: &DbActor,
    offer_activity_id: &str,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_accept_offer(
        instance.uri_str(),
        &sender.profile.username,
        subscription_option,
        invoice,
        &remote_actor.id,
        offer_activity_id,
    )?;
    let recipients = Recipient::for_inbox(remote_actor);
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;
    use apx_core::caip2::ChainId;
    use serde_json::{json, to_value};
    use mitra_models::invoices::types::DbChainId;
    use super::*;

    #[test]
    fn test_build_accept_offer() {
        let instance_uri = "https://local.example";
        let sender_username = "proposer";
        let subscription_option = MoneroSubscription {
            chain_id: ChainId::monero_mainnet(),
            price: NonZeroU64::new(20000).unwrap(),
            payout_address: "test".to_string(),
        };
        let invoice_id = "edc374aa-e580-4a58-9404-f3e8bf8556b2".parse().unwrap();
        let invoice = Invoice {
            id: invoice_id,
            chain_id: DbChainId::new(&subscription_option.chain_id),
            payment_address: Some("8xyz".to_string()),
            amount: 60000000,
            ..Default::default()
        };
        let remote_actor_id = "https://remote.example/users/payer";
        let offer_activity_id = "https://remote.example/activities/123";
        let activity = build_accept_offer(
            instance_uri,
            sender_username,
            &subscription_option,
            &invoice,
            remote_actor_id,
            offer_activity_id,
        ).unwrap();

        assert_eq!(activity.actor, "https://local.example/users/proposer");
        assert_eq!(activity.activity_type, "Accept");
        assert_eq!(activity.object, offer_activity_id);
        assert_eq!(activity.to, "https://remote.example/users/payer");
        let expected_agreement = json!({
            "type": "Agreement",
            "id": "https://local.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2",
            "attributedTo": "https://local.example/users/proposer",
            "stipulates": {
                "id": "https://local.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2#primary",
                "type": "Commitment",
                "satisfies": "https://local.example/users/proposer/proposals/monero:418015bb9ae982a1975da7d79277c270#primary",
                "resourceQuantity": {
                    "hasUnit": "second",
                    "hasNumericalValue": "3000",
                },
            },
            "stipulatesReciprocal": {
                "id": "https://local.example/objects/agreements/edc374aa-e580-4a58-9404-f3e8bf8556b2#reciprocal",
                "type": "Commitment",
                "satisfies": "https://local.example/users/proposer/proposals/monero:418015bb9ae982a1975da7d79277c270#reciprocal",
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
            to_value(activity.result).unwrap(),
            expected_agreement,
        );
    }
}
