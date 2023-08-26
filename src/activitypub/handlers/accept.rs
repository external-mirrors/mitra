use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;
use mitra_models::{
    database::DatabaseClient,
    profiles::queries::get_profile_by_remote_actor_id,
    relationships::queries::{
        follow_request_accepted,
        get_follow_request_by_id,
    },
    relationships::types::FollowRequestStatus,
};
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    deserialization::deserialize_into_object_id,
    identifiers::parse_local_object_id,
    vocabulary::FOLLOW,
};

use super::HandlerResult;

#[derive(Deserialize)]
struct Accept {
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_accept(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    // Accept(Follow)
    let activity: Accept = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let follow_request_id = parse_local_object_id(
        &config.instance_url(),
        &activity.object,
    )?;
    let follow_request = get_follow_request_by_id(db_client, &follow_request_id).await?;
    if follow_request.target_id != actor_profile.id {
        return Err(ValidationError("actor is not a target").into());
    };
    if matches!(follow_request.request_status, FollowRequestStatus::Accepted) {
        // Ignore Accept if follow request already accepted
        return Ok(None);
    };
    follow_request_accepted(db_client, &follow_request_id).await?;
    Ok(Some(FOLLOW))
}
