use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    deserialization::deserialize_into_object_id,
    url::is_same_origin,
    utils::is_activity,
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    invoices::queries::{
        get_remote_invoice_by_object_id,
        set_invoice_status,
    },
    invoices::types::InvoiceStatus,
    posts::queries::{
        get_remote_post_by_object_id,
        set_pinned_flag,
    },
    profiles::queries::get_remote_profile_by_actor_id,
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
    agent::build_federation_agent,
    authentication::{verify_signed_activity, AuthenticationError},
    identifiers::parse_local_actor_id,
    importers::fetch_any_object,
    ownership::verify_activity_owner,
    vocabulary::{DISLIKE, LIKE},
};

use super::{
    like::handle_like,
    Descriptor,
    HandlerResult,
};

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

#[derive(Deserialize)]
struct ConversationAdd {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,

    object: JsonValue,

    #[serde(deserialize_with = "deserialize_into_object_id")]
    target: String,
}

// https://fediversity.site/help/develop/en/Containers
async fn handle_fep_171b_add(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    add: JsonValue,
) -> HandlerResult {
    let ConversationAdd {
        actor: conversation_owner,
        object: mut activity,
        target,
    } =
        serde_json::from_value(add)
            .map_err(|_| ValidationError("unexpected activity structure"))?;
    // Authentication
    match verify_signed_activity(
        config,
        db_client,
        &activity,
        false, // fetch signer
    ).await {
        Ok(_) => (),
        Err(AuthenticationError::NoJsonSignature) => {
            // Verify activity by fetching it from origin
            let activity_id = activity["id"].as_str()
                .ok_or(ValidationError("unexpected activity structure"))?
                .to_owned();
            let instance = config.instance();
            let agent = build_federation_agent(&instance, None);
            match fetch_any_object(&agent, &activity_id).await {
                Ok(activity_fetched) => {
                    log::info!("fetched activity {}", activity_id);
                    activity = activity_fetched
                },
                Err(error) => {
                    // Wrapped activities are not always available
                    log::warn!("failed to fetch activity ({error}): {activity_id}");
                    return Ok(None);
                },
            };
        },
        Err(AuthenticationError::DatabaseError(db_error)) => return Err(db_error.into()),
        Err(other_error) => {
            log::warn!("{other_error}");
            return Err(ValidationError("invalid integrity proof").into());
        },
    };
    // Authorization
    if !is_same_origin(&conversation_owner, &target)
        .map_err(|_| ValidationError("invalid object ID"))?
    {
        return Err(ValidationError("actor is not allowed to modify target").into());
    };
    verify_activity_owner(&activity)?;
    if let Some(context) = activity["context"].as_str() {
        if context != target {
            log::warn!("context doesn't match Add target");
        };
    } else {
        log::warn!("'context' is missing");
    };

    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("unexpected activity structure"))?
        .to_owned();
    if let LIKE | DISLIKE = activity_type.as_str() {
        let maybe_type = handle_like(config, db_client, activity).await?;
        Ok(maybe_type.map(|_| Descriptor::object(activity_type)))
    } else {
        log::warn!("activity is not supported: Add({activity_type})");
        Ok(None)
    }
}

pub async fn handle_add(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    if is_activity(&activity["object"]) {
        return handle_fep_171b_add(config, db_client, activity).await;
    };
    let activity: Add = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_remote_profile_by_actor_id(
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
        subscribe_opt(db_client, sender.id, recipient.id).await?;

        // FEP-0837 confirmation
        let subscription_expires_at = activity.end_time
            .ok_or(ValidationError("'endTime' property is missing"))?;
        match activity.context {
            Some(ref agreement_id) => {
                match get_remote_invoice_by_object_id(
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
                            return Ok(Some(Descriptor::target("subscribers")));
                        };
                        set_invoice_status(
                            db_client,
                            invoice.id,
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
            sender.id,
            recipient.id,
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
                    sender,
                    recipient,
                );
            },
            Err(DatabaseError::NotFound(_)) => {
                create_subscription(
                    db_client,
                    sender.id,
                    recipient.id,
                    subscription_expires_at,
                    Utc::now(),
                ).await?;
                log::info!(
                    "subscription created: {0} to {1}",
                    sender,
                    recipient,
                );
            },
            Err(other_error) => return Err(other_error.into()),
        };
        return Ok(Some(Descriptor::target("subscribers")));
    };
    if Some(activity.target.clone()) == actor.featured {
        // Add to featured
        let post = match get_remote_post_by_object_id(
            db_client,
            &activity.object,
        ).await {
            Ok(post) => post,
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        set_pinned_flag(db_client, post.id, true).await?;
        return Ok(Some(Descriptor::target("featured")));
    };
    log::warn!("unknown target: {}", activity.target);
    Ok(None)
}
