use apx_sdk::deserialization::deserialize_into_object_id;
use serde::Deserialize;
use serde_json::Value;

use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    profiles::queries::get_remote_profile_by_actor_id,
    relationships::queries::{
        follow_request_rejected,
        unfollow,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    identifiers::canonicalize_id,
    importers::ApClient,
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
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: Value,
) -> HandlerResult {
    // Reject(Follow)
    let reject: Reject = serde_json::from_value(activity)?;
    let db_client = &mut **get_database_client(db_pool).await?;
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &reject.actor,
    ).await?;
    let canonical_object_id = canonicalize_id(&reject.object)?;
    let follow_request = match get_follow_request_by_activity_id(
        db_client,
        ap_client.instance.uri_str(),
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
