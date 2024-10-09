use serde::Deserialize;
use serde_json::Value;

use apx_sdk::deserialization::deserialize_into_object_id;
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    notifications::helpers::{
        create_subscription_expiration_notification,
    },
    posts::queries::{
        get_remote_post_by_object_id,
        set_pinned_flag,
    },
    profiles::queries::get_remote_profile_by_actor_id,
    relationships::queries::unsubscribe,
    users::queries::get_user_by_name,
};
use mitra_validators::errors::ValidationError;

use crate::{
    identifiers::parse_local_actor_id,
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Remove {
    actor: String,

    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    target: String,
}

pub async fn handle_remove(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: Remove = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let actor = actor_profile.actor_json
        .expect("actor data should be present");
    if Some(activity.target.clone()) == actor.subscribers {
        // Removing from subscribers
        let username = parse_local_actor_id(
            &config.instance_url(),
            &activity.object,
        )?;
        let user = get_user_by_name(db_client, &username).await?;
        // actor is recipient, user is sender
        match unsubscribe(db_client, user.id, actor_profile.id).await {
            Ok(_) => {
                create_subscription_expiration_notification(
                    db_client,
                    actor_profile.id,
                    user.id,
                ).await?;
                return Ok(Some(Descriptor::target("subscribers")));
            },
            // Ignore removal if relationship does not exist
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
    };
    if Some(activity.target) == actor.featured {
        // Remove from featured
        let post = match get_remote_post_by_object_id(
            db_client,
            &activity.object,
        ).await {
            Ok(post) => post,
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        set_pinned_flag(db_client, post.id, false).await?;
        return Ok(Some(Descriptor::target("featured")));
    };
    Ok(None)
}
