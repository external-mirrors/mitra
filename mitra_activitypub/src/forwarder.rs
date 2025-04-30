use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    deserialization::deserialize_into_id_array,
    url::Url,
    utils::is_public,
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

pub fn get_activity_audience(
    activity: &JsonValue,
) -> Result<Vec<Url>, ValidationError> {
    let activity: ActivityAudience = serde_json::from_value(activity.clone())
        .map_err(|_| ValidationError("invalid audience"))?;
    let audience = [activity.to, activity.cc].concat()
        .iter()
        .filter(|target_id| !is_public(target_id))
        .map(|id| canonicalize_id(id))
        .collect::<Result<_, _>>()?;
    Ok(audience)
}

pub async fn get_activity_remote_recipients(
    db_client: &impl DatabaseClient,
    activity: &JsonValue,
) -> Result<Vec<DbActor>, HandlerError> {
    let audience = get_activity_audience(activity)?;
    let mut recipients = vec![];
    for target_id in audience {
        // TODO: FEP-EF61: followers collections
        let profile = match get_remote_profile_by_actor_id(
            db_client,
            &target_id.to_string(),
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

#[cfg(test)]
mod tests {
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
        let audience = get_activity_audience(&activity).unwrap();
        assert_eq!(audience.len(), 1);
        assert_eq!(
            audience[0].to_string(),
            "https://social.example/users/1/followers",
        );
    }
}
