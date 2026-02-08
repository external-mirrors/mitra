use std::fmt;

use apx_sdk::{
    core::url::canonical::CanonicalUri,
    deserialization::{
        deserialize_into_id_array,
        object_to_id,
    },
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    activitypub::queries::{
        add_object_to_collection,
        save_activity,
    },
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    users::queries::{
        get_portable_user_by_actor_id,
        get_portable_user_by_id,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    forwarder::{
        get_activity_recipients,
        EndpointType,
    },
    identifiers::canonicalize_id,
    importers::ApClient,
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
    note::normalize_audience,
    offer::handle_offer,
    reject::handle_reject,
    remove::handle_remove,
    undo::handle_undo,
    update::handle_update,
    HandlerError,
};

const FORWARDER_LIMIT: usize = 50;

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

#[derive(Deserialize)]
struct ActivityAudience {
    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    to: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    cc: Vec<String>,
}

pub fn get_activity_audience(
    activity: &JsonValue,
    // implicit audience
    maybe_recipient_id: Option<&str>,
) -> Result<Vec<CanonicalUri>, ValidationError> {
    let activity: ActivityAudience = serde_json::from_value(activity.clone())
        .map_err(|_| ValidationError("invalid audience"))?;
    let mut audience = [activity.to, activity.cc].concat();
    if let Some(recipient_id) = maybe_recipient_id {
        audience.push(recipient_id.to_owned());
    };
    if audience.is_empty() {
        log::warn!("activity audience is not known");
    };
    // Targets will be sorted
    let audience = normalize_audience(&audience)?;
    Ok(audience)
}

pub async fn handle_activity(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    activity: &JsonValue,
    is_authenticated: bool,
    maybe_recipient_id: Option<&str>,
    maybe_sender_id: Option<&str>,
) -> Result<String, HandlerError> {
    let ap_client = ApClient::new_with_pool(config, db_pool).await?;
    let activity = if is_authenticated {
        activity.clone()
    } else {
        let activity_id = get_object_id(activity)?;
        let activity_type = activity["type"].as_str()
            .ok_or(ValidationError("'type' property is missing"))?;
        match activity_type {
            CREATE | DELETE => {
                // Object will be fetched in the handler
                activity.clone()
            },
            _ => {
                // Fetch activity
                let activity_fetched = ap_client.fetch_object(activity_id).await?;
                log::info!("fetched activity {activity_id}");
                activity_fetched
            },
        }
    };

    // Validate common activity attributes
    verify_activity_owner(&activity)?;
    let activity_id = get_object_id(&activity)?;
    let canonical_activity_id = canonicalize_id(activity_id)?;
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?
        .to_owned();
    let activity_actor = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid actor property"))?;
    let canonical_actor_id = canonicalize_id(&activity_actor)?;
    let audience = get_activity_audience(&activity, maybe_recipient_id)?;

    let activity_clone = activity.clone();
    let maybe_descriptor = match activity_type.as_str() {
        ACCEPT => {
            handle_accept(config, db_pool, activity).await?
        },
        ADD => {
            handle_add(config, db_pool, activity).await?
        },
        ANNOUNCE => {
            handle_announce(config, db_pool, activity).await?
        },
        BLOCK => {
            handle_block(config, db_pool, activity).await?
        },
        CREATE => {
            handle_create(
                config,
                db_pool,
                activity,
                maybe_sender_id,
                is_authenticated,
            ).await?
        },
        DELETE => {
            handle_delete(config, db_pool, activity).await?
        },
        FOLLOW => {
            handle_follow(config, db_pool, activity).await?
        },
        DISLIKE | LIKE | EMOJI_REACT => {
            handle_like(config, db_pool, activity).await?
        },
        LISTEN => {
            None // ignore
        },
        MOVE => {
            handle_move(config, db_pool, activity).await?
        },
        OFFER => {
            handle_offer(config, db_pool, activity).await?
        },
        REJECT => {
            handle_reject(config, db_pool, activity).await?
        },
        REMOVE => {
            handle_remove(config, db_pool, activity).await?
        },
        UNDO => {
            handle_undo(config, db_pool, activity).await?
        },
        UPDATE => {
            handle_update(config, db_pool, activity, is_authenticated).await?
        },
        _ => {
            log::warn!("activity type is not supported: {}", activity);
            None
        },
    };
    if let Some(descriptor) = maybe_descriptor {
        let db_client = &**get_database_client(db_pool).await?;
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
        // TODO: remove limit
        for recipient in recipients.iter().take(FORWARDER_LIMIT) {
            // Recipient is a local actor: add activity to its inbox
            // and forward to other gateways
            if recipient.has_portable_account() {
                if !is_new_activity {
                    log::warn!("activity has already been forwarded from inbox");
                    continue;
                };
                let recipient = get_portable_user_by_id(
                    db_client,
                    recipient.id,
                ).await?;
                let recipient_actor_data =
                    recipient.profile.expect_actor_data();
                add_object_to_collection(
                    db_client,
                    recipient.id,
                    &recipient_actor_data.inbox,
                    &canonical_activity_id.to_string(),
                ).await?;
                // Forward
                if let Some(job_data) = OutgoingActivityJobData::new_forwarded(
                    config.instance().uri_str(),
                    &recipient,
                    &activity_clone,
                    vec![],
                    EndpointType::Inbox,
                ) {
                    // Activity has already been saved
                    job_data.enqueue(db_client).await?;
                } else {
                    log::warn!("signing keys are not found in actor document");
                };
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
                    config.instance().uri_str(),
                    &actor,
                    &activity_clone,
                    remote_recipients,
                    EndpointType::Outbox,
                ) {
                    // Activity has already been saved
                    job_data.enqueue(db_client).await?;
                } else {
                    log::warn!("signing keys are not found in actor document");
                };
            },
            Ok(_) => {
                log::warn!("activity has already been added to outbox");
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

#[cfg(test)]
mod tests {
    use apx_sdk::constants::AP_PUBLIC;
    use serde_json::json;
    use super::*;

    #[test]
    fn test_get_activity_audience() {
        let activity = json!({
            "id": "https://social.example/activities/123",
            "type": "Announce",
            "actor": "https://social.example/users/1",
            "object": "https://social.example/objects/321",
            "to": "as:Public",
            "cc": "https://social.example/users/1/followers",
        });
        let audience = get_activity_audience(&activity, None).unwrap();
        assert_eq!(audience.len(), 2);
        assert_eq!(
            audience[0].to_string(),
            "https://social.example/users/1/followers",
        );
        assert_eq!(audience[1].to_string(), AP_PUBLIC);
    }
}
