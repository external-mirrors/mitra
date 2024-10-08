use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    deserialization::deserialize_into_id_array,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::{
        queries::get_remote_profile_by_actor_id,
        types::DbActor,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    errors::HandlerError,
    identifiers::canonicalize_id,
};

#[derive(Deserialize)]
struct ActivityAudience {
    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    to: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    cc: Vec<String>,
}

pub async fn get_activity_remote_recipients(
    db_client: &impl DatabaseClient,
    activity: &JsonValue,
) -> Result<Vec<DbActor>, HandlerError> {
    let activity: ActivityAudience = serde_json::from_value(activity.clone())
        .map_err(|_| ValidationError("invalid audience"))?;
    let audience = [activity.to, activity.cc].concat();
    let mut recipients = vec![];
    for target_id in audience {
        // TODO: FEP-EF61: followers collections
        let canonical_target_id = canonicalize_id(&target_id)?;
        let profile = match get_remote_profile_by_actor_id(
            db_client,
            &canonical_target_id.to_string(),
        ).await {
            Ok(profile) => profile,
            Err(DatabaseError::NotFound(_)) => continue,
            Err(other_error) => return Err(other_error.into()),
        };
        let actor_data = profile.expect_actor_data();
        recipients.push(actor_data.clone());
    };
    Ok(recipients)
}
