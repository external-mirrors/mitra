use std::fmt;

use apx_sdk::deserialization::object_to_id;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    activitypub::queries::{
        add_object_to_collection,
        save_activity,
    },
    database::{DatabaseClient, DatabaseError},
    users::queries::get_portable_user_by_actor_id,
};
use mitra_validators::errors::ValidationError;

use crate::{
    forwarder::{get_activity_audience, get_activity_recipients},
    identifiers::canonicalize_id,
    ownership::{get_object_id, verify_activity_owner},
    queues::OutgoingActivityJobData,
    vocabulary::*,
};

use super::{
    accept::handle_accept,
    add::handle_add,
    announce::handle_announce,
    block::handle_block,
    create::handle_create,
    delete::handle_delete,
    follow::handle_follow,
    like::handle_like,
    r#move::handle_move,
    offer::handle_offer,
    reject::handle_reject,
    remove::handle_remove,
    undo::handle_undo,
    update::handle_update,
    HandlerError,
};

pub enum Descriptor {
    Object(String),
    Target(String),
}

impl Descriptor {
    pub fn object(object_type: impl ToString) -> Self {
        Self::Object(object_type.to_string())
    }

    pub fn target(target_prop: &'static str) -> Self {
        Self::Target(target_prop.to_string())
    }
}

impl fmt::Display for Descriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Object(object) => write!(formatter, "{object}"),
            Self::Target(target) => write!(formatter, "target: {target}"),
        }
    }
}

pub async fn handle_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: &JsonValue,
    is_authenticated: bool,
    maybe_recipient_id: Option<&str>,
    maybe_sender_id: Option<&str>,
) -> Result<String, HandlerError> {
    // Validate common activity attributes
    verify_activity_owner(activity)?;
    let activity_id = get_object_id(activity)?;
    let canonical_activity_id = canonicalize_id(activity_id)?;
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?
        .to_owned();
    let activity_actor = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid actor property"))?;
    let canonical_actor_id = canonicalize_id(&activity_actor)?;
    let audience = get_activity_audience(activity, maybe_recipient_id)?;

    let activity = activity.clone();
    let activity_clone = activity.clone();
    let maybe_descriptor = match activity_type.as_str() {
        ACCEPT => {
            handle_accept(config, db_client, activity).await?
        },
        ADD => {
            handle_add(config, db_client, activity).await?
        },
        ANNOUNCE => {
            handle_announce(config, db_client, activity).await?
        },
        BLOCK => {
            handle_block(config, db_client, activity).await?
        },
        CREATE => {
            handle_create(
                config,
                db_client,
                activity,
                maybe_sender_id,
                is_authenticated,
            ).await?
        },
        DELETE => {
            handle_delete(config, db_client, activity).await?
        },
        FOLLOW => {
            handle_follow(config, db_client, activity).await?
        },
        DISLIKE | LIKE | EMOJI_REACT => {
            handle_like(config, db_client, activity).await?
        },
        LISTEN => {
            None // ignore
        },
        MOVE => {
            handle_move(config, db_client, activity).await?
        },
        OFFER => {
            handle_offer(config, db_client, activity).await?
        },
        REJECT => {
            handle_reject(config, db_client, activity).await?
        },
        REMOVE => {
            handle_remove(config, db_client, activity).await?
        },
        UNDO => {
            handle_undo(config, db_client, activity).await?
        },
        UPDATE => {
            handle_update(config, db_client, activity, is_authenticated).await?
        },
        _ => {
            log::warn!("activity type is not supported: {}", activity);
            None
        },
    };
    if let Some(descriptor) = maybe_descriptor {
        let is_new_activity = save_activity(
            db_client,
            &canonical_activity_id,
            &activity_clone,
        ).await?;
        // Remote recipients
        let recipients = get_activity_recipients(
            db_client,
            &audience,
        ).await?;
        for recipient in recipients.iter() {
            // Recipient is a local actor: add activity to its inbox
            if recipient.has_account() && recipient.is_portable() {
                add_object_to_collection(
                    db_client,
                    recipient.id,
                    &recipient.expect_actor_data().inbox,
                    &canonical_activity_id.to_string(),
                ).await?;
            };
        };
        match get_portable_user_by_actor_id(
            db_client,
            &canonical_actor_id.to_string(),
        ).await {
            Ok(actor) if is_new_activity => {
                // Activity has been performed by a local actor:
                // add to outbox and forward
                add_object_to_collection(
                    db_client,
                    actor.id,
                    &actor.profile.expect_actor_data().outbox,
                    &canonical_activity_id.to_string(),
                ).await?;
                let remote_recipients = recipients.iter()
                    .filter_map(|recipient| recipient.actor_json.clone())
                    .collect();
                // Forward only if HTTP signature can be created
                if let Some(job_data) = OutgoingActivityJobData::new_forwarded(
                    &config.instance_url(),
                    &actor,
                    &activity_clone,
                    remote_recipients,
                ) {
                    // Activity has already been saved
                    job_data.enqueue(db_client).await?;
                } else {
                    log::warn!("signing keys are not found in actor document");
                };
            },
            Ok(_) => {
                log::warn!("activity has already been forwarded");
            },
            Err(DatabaseError::NotFound(_)) => (),
            Err(other_error) => return Err(other_error.into()),
        };
        log::info!(
            "processed {}({}) from {}",
            activity_type,
            descriptor,
            activity_actor,
        );
    };
    Ok(canonical_activity_id.to_string())
}
