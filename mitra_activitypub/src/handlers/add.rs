use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;
use mitra_federation::deserialization::deserialize_into_object_id;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    invoices::queries::{
        get_invoice_by_remote_object_id,
        set_invoice_status,
    },
    invoices::types::InvoiceStatus,
    posts::queries::{
        get_post_by_remote_object_id,
        set_pinned_flag,
    },
    profiles::queries::get_profile_by_remote_actor_id,
    relationships::queries::subscribe_opt,
    subscriptions::queries::{
        create_subscription,
        get_subscription_by_participants,
        update_subscription,
    },
    users::queries::get_user_by_name,
};
use mitra_validators::errors::ValidationError;

use crate::{
    identifiers::parse_local_actor_id,
    vocabulary::{NOTE, PERSON},
};

use super::HandlerResult;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Add {
    actor: String,

    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    target: String,

    end_time: Option<DateTime<Utc>>,
    context: Option<String>,
}

pub async fn handle_add(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: Add = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let actor = actor_profile.actor_json.as_ref()
        .expect("actor data should be present");
    if Some(activity.target.clone()) == actor.subscribers {
        // Adding to subscribers
        let username = parse_local_actor_id(
            &config.instance_url(),
            &activity.object,
        )?;
        let sender = get_user_by_name(db_client, &username).await?;
        let recipient = actor_profile;
        subscribe_opt(db_client, &sender.id, &recipient.id).await?;

        // FEP-0837 confirmation
        let subscription_expires_at = match activity.end_time {
            Some(subscription_expires_at) => subscription_expires_at,
            // FEP-0837 confirmation not implemented, return
            _ => return Ok(Some(PERSON)),
        };
        match activity.context {
            Some(ref agreement_id) => {
                match get_invoice_by_remote_object_id(
                    db_client,
                    agreement_id,
                ).await {
                    Ok(invoice) => {
                        // FEP-0837 confirmation
                        if invoice.sender_id != sender.id || invoice.recipient_id != recipient.id {
                            return Err(ValidationError("invalid context ID").into());
                        };
                        if invoice.invoice_status == InvoiceStatus::Completed {
                            // Activity has been already processed
                            return Ok(Some(PERSON));
                        };
                        set_invoice_status(
                            db_client,
                            &invoice.id,
                            InvoiceStatus::Completed,
                        ).await?;
                    },
                    Err(DatabaseError::NotFound(_)) => {
                        // Payment initiated via payment page (no FEP-0837)
                        log::warn!("unknown agreement");
                    },
                    Err(other_error) => return Err(other_error.into()),
                };
            },
            _ => log::warn!("no agreement"),
        };

        match get_subscription_by_participants(
            db_client,
            &sender.id,
            &recipient.id,
        ).await {
            Ok(subscription) => {
                update_subscription(
                    db_client,
                    subscription.id,
                    subscription_expires_at,
                    Utc::now(),
                ).await?;
                log::info!(
                    "subscription updated: {0} to {1}",
                    subscription.sender_id,
                    subscription.recipient_id,
                );
            },
            Err(DatabaseError::NotFound(_)) => {
                create_subscription(
                    db_client,
                    sender.id,
                    None, // matching by address is not required
                    recipient.id,
                    None, // chain ID is not required
                    subscription_expires_at,
                    Utc::now(),
                ).await?;
                log::info!(
                    "subscription created: {0} to {1}",
                    sender.id,
                    recipient.id,
                );
            },
            Err(other_error) => return Err(other_error.into()),
        };
        return Ok(Some(PERSON));
    };
    if Some(activity.target.clone()) == actor.featured {
        // Add to featured
        let post = match get_post_by_remote_object_id(
            db_client,
            &activity.object,
        ).await {
            Ok(post) => post,
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        set_pinned_flag(db_client, &post.id, true).await?;
        return Ok(Some(NOTE));
    };
    log::warn!("unknown target: {}", activity.target);
    Ok(None)
}
