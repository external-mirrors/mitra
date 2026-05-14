use apx_sdk::core::url::http_uri::HttpUri;
use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::DatabaseError,
    invoices::types::Invoice,
    profiles::types::{DbActor, MoneroSubscription},
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::{
    contexts::Context,
    deliverer::Recipient,
    identifiers::{
        local_activity_id,
        local_actor_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::UPDATE,
};

use super::{
    agreement::{build_agreement, Agreement},
    proposal::build_valueflows_context,
};

#[derive(Serialize)]
struct UpdateAgreement {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Agreement,

    to: Vec<String>,
}

fn build_update_agreement(
    instance_uri: &HttpUri,
    sender_username: &str,
    remote_payer_id: &str,
    subscription_option: &MoneroSubscription,
    invoice: &Invoice,
) -> Result<UpdateAgreement, DatabaseError> {
    let activity_id = local_activity_id(
        instance_uri.as_str(),
        UPDATE,
        generate_ulid(),
    );
    let actor_id = local_actor_id(
        instance_uri.as_str(),
        sender_username,
    );
    let agreement = build_agreement(
        instance_uri.as_str(),
        sender_username,
        subscription_option,
        invoice,
    )?;
    let activity = UpdateAgreement {
        _context: build_valueflows_context(),
        activity_type: UPDATE.to_string(),
        id: activity_id,
        actor: actor_id,
        object: agreement,
        to: vec![remote_payer_id.to_owned()],
    };
    Ok(activity)
}

pub fn prepare_update_agreement(
    instance: &Instance,
    sender: &User,
    subscription_option: &MoneroSubscription,
    invoice: &Invoice,
    remote_payer: &DbActor,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_update_agreement(
        instance.uri(),
        &sender.profile.username,
        &remote_payer.id,
        subscription_option,
        invoice,
    )?;
    let recipients = Recipient::for_inbox(remote_payer);
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
    use mitra_models::{
        invoices::types::{DbChainId, InvoiceStatus},
        profiles::types::DbActorProfile,
    };
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_update_agreement() {
        let instance_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let sender = User {
            profile: DbActorProfile::local_for_test("testuser"),
            ..Default::default()
        };
        let remote_actor_id = "https://remote.example/users/payer";
        let subscription_option = MoneroSubscription {
            chain_id: ChainId::monero_mainnet(),
            price: NonZeroU64::new(20000).unwrap(),
        };
        let invoice_id = "edc374aa-e580-4a58-9404-f3e8bf8556b2".parse().unwrap();
        let invoice = Invoice {
            id: invoice_id,
            chain_id: DbChainId::new(&subscription_option.chain_id),
            invoice_status: InvoiceStatus::Paid,
            amount: 60000000,
            payment_address: Some("8xyz".to_string()),
            ..Default::default()
        };
        let activity = build_update_agreement(
            &instance_uri,
            &sender.profile.username,
            remote_actor_id,
            &subscription_option,
            &invoice,
        ).unwrap();
        assert_eq!(activity.activity_type, UPDATE);
        assert_eq!(
            activity.object.id.unwrap(),
            format!("{instance_uri}/objects/agreements/{invoice_id}"),
        );
        assert_eq!(
            activity.object.attributed_to.unwrap(),
            format!("{instance_uri}/users/testuser"),
        );
        assert_eq!(
            activity.object.preview.unwrap().name,
            "Paid",
        );
        assert_eq!(activity.to, vec![remote_actor_id.to_owned()]);
    }
}
