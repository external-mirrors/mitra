use serde::Deserialize;
use serde_json::Value;

use mitra_activitypub::deserialization::deserialize_into_object_id;
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::get_profile_by_remote_actor_id,
    relationships::queries::{
        follow_request_rejected,
        get_follow_request_by_id,
        unfollow,
    },
};
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    identifiers::parse_local_object_id,
    vocabulary::FOLLOW,
};

use super::HandlerResult;

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
    let activity: Reject = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let follow_request_id = parse_local_object_id(
        &config.instance_url(),
        &activity.object,
    )?;
    let follow_request = match get_follow_request_by_id(
        db_client,
        &follow_request_id,
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
    follow_request_rejected(db_client, &follow_request.id).await?;
    // Delete follow request, and delete relationship too.
    // Reject() activity might be used to remove followers.
    match unfollow(
        db_client,
        &follow_request.source_id,
        &follow_request.target_id,
    ).await {
        Ok(_) | Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(FOLLOW))
}
