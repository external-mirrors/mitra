use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;
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

use crate::activitypub::{
    identifiers::parse_local_actor_id,
    vocabulary::{NOTE, PERSON},
};

use super::{HandlerError, HandlerResult};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Add {
    actor: String,
    object: String,
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
    let actor = actor_profile.actor_json.ok_or(HandlerError::LocalObject)?;
    if Some(activity.target.clone()) == actor.subscribers {
        // Adding to subscribers
        let username = parse_local_actor_id(
            &config.instance_url(),
            &activity.object,
        )?;
        let user = get_user_by_name(db_client, &username).await?;
        subscribe_opt(db_client, &user.id, &actor_profile.id).await?;

        // FEP-0837 confirmation
        let (invoice, subscription_expires_at) = match (
            activity.context,
            activity.end_time,
        ) {
            (Some(ref agreement_id), Some(subscription_expires_at)) => {
                let invoice = get_invoice_by_remote_object_id(
                    db_client,
                    agreement_id,
                ).await?;
                (invoice, subscription_expires_at)
            },
            // FEP-0837 confirmation not implemented, return
            _ => return Ok(Some(PERSON)),
        };
        if invoice.sender_id != user.id || invoice.recipient_id != actor_profile.id {
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

        match get_subscription_by_participants(
            db_client,
            &invoice.sender_id,
            &invoice.recipient_id,
        ).await {
            Ok(subscription) => {
                update_subscription(
                    db_client,
                    subscription.id,
                    &subscription_expires_at,
                    &Utc::now(),
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
                    &user.id,
                    None, // matching by address is not required
                    &actor_profile.id,
                    invoice.chain_id.inner(),
                    &subscription_expires_at,
                    &Utc::now(),
                ).await?;
                log::info!(
                    "subscription created: {0} to {1}",
                    invoice.sender_id,
                    invoice.recipient_id,
                );
            },
            Err(other_error) => return Err(other_error.into()),
        };
        return Ok(Some(PERSON));
    };
    if Some(activity.target) == actor.featured {
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
    Ok(None)
}
