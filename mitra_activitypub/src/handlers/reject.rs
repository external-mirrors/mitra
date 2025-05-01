use serde::Deserialize;
use serde_json::Value;

use apx_sdk::deserialization::deserialize_into_object_id;
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::get_remote_profile_by_actor_id,
    relationships::queries::{
        follow_request_rejected,
        unfollow,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    identifiers::canonicalize_id,
    vocabulary::FOLLOW,
};

use super::{
    accept::get_follow_request_by_activity_id,
    Descriptor,
    HandlerResult,
};

#[derive(Deserialize)]
struct Reject {
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_reject(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    // Reject(Follow)
    let activity: Reject = serde_json::from_value(activity)?;
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let canonical_object_id = canonicalize_id(&activity.object)?;
    let follow_request = match get_follow_request_by_activity_id(
        db_client,
        &config.instance_url(),
        &canonical_object_id.to_string(),
    ).await {
        Ok(follow_request) => follow_request,
        Err(DatabaseError::NotFound(_)) => {
            // Ignore Reject if follow request has already been rejected
            return Ok(None);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    if follow_request.target_id != actor_profile.id {
        return Err(ValidationError("actor is not a target").into());
    };
    follow_request_rejected(db_client, follow_request.id).await?;
    // Delete follow request, and delete relationship too.
    // Reject() activity might be used to remove followers.
    match unfollow(
        db_client,
        follow_request.source_id,
        follow_request.target_id,
    ).await {
        Ok(_) | Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(Descriptor::object(FOLLOW)))
}
