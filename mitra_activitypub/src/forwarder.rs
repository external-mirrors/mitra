use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    core::{
        crypto::common::PublicKey,
        crypto_eddsa::ed25519_public_key_from_secret_key,
        crypto_rsa::RsaPublicKey,
        url::canonical::Url,
    },
    deserialization::deserialize_into_id_array,
    utils::{is_public, is_verification_method},
};
use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
    users::types::PortableUser,
};
use mitra_validators::errors::ValidationError;

use crate::{
    identifiers::canonicalize_id,
    importers::get_profile_by_actor_id,
    keys::verification_method_to_public_key,
    ownership::is_local_origin,
};

fn find_objects(object: &JsonValue) -> Vec<JsonValue> {
    let mut objects = vec![];
    match object {
        JsonValue::Object(map) => {
            objects.push(object.clone());
            for (_key, value) in map {
                let embedded = find_objects(value);
                objects.extend(embedded);
            };
        },
        JsonValue::Array(array) => {
            for value in array {
                let embedded = find_objects(value);
                objects.extend(embedded);
            };
        },
        _ => (),
    };
    objects
}

pub fn validate_public_keys(
    instance: &Instance,
    maybe_account: Option<&PortableUser>,
    object: &JsonValue,
) -> Result<(), ValidationError> {
    let objects = find_objects(object);
    for object in objects {
        if !is_verification_method(&object) {
            continue;
        };
        let Some(object_id) = object["id"].as_str() else {
            continue;
        };
        if !is_local_origin(instance, object_id) {
            continue;
        };
        let public_key = verification_method_to_public_key(object)?;
        // Local public keys must be known to the server
        let is_known = match public_key {
            PublicKey::Ed25519(ed25519_public_key) => {
                maybe_account
                    .map(|account| ed25519_public_key_from_secret_key(&account.ed25519_secret_key))
                    .is_some_and(|key| key == ed25519_public_key)
            },
            PublicKey::Rsa(rsa_public_key) => {
                maybe_account
                    .map(|account| RsaPublicKey::from(&account.rsa_secret_key))
                    .is_some_and(|key| key == rsa_public_key)
            },
        };
        if !is_known {
            return Err(ValidationError("unexpected public key"));
        };
    };
    Ok(())
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
    maybe_recipient_id: Option<&str>,
) -> Result<Vec<Url>, ValidationError> {
    let activity: ActivityAudience = serde_json::from_value(activity.clone())
        .map_err(|_| ValidationError("invalid audience"))?;
    let mut audience = [activity.to, activity.cc].concat();
    if let Some(recipient_id) = maybe_recipient_id {
        audience.push(recipient_id.to_owned());
    };
    if audience.is_empty() {
        log::warn!("activity audience is not known");
    };
    let audience: Vec<_> = audience
        .iter()
        .filter(|target_id| !is_public(target_id))
        .map(|id| canonicalize_id(id))
        .collect::<Result<_, _>>()?;
    Ok(audience)
}

pub async fn get_activity_recipients(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    audience: &[Url],
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let mut targets = vec![];
    const RECIPIENT_LIMIT: usize = 50;
    // TODO: single database query
    for target_id in audience.iter().take(RECIPIENT_LIMIT) {
        // TODO: expand collections
        let target = match get_profile_by_actor_id(
            db_client,
            instance_url,
            &target_id.to_string(),
        ).await {
            Ok(profile) => profile,
            Err(DatabaseError::NotFound(_)) => continue,
            Err(other_error) => return Err(other_error),
        };
        targets.push(target);
    };
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_find_objects() {
        let activity = json!({
            "id": "https://social.example/activities/123",
            "type": "Update",
            "actor": "https://social.example/actors/1",
            "object": {
                "id": "https://social.example/actors/1",
                "type": "Person",
                "publicKey": {
                    "id": "https://social.example/actors/1#main-key",
                    "publicKeyPem": "",
                },
                "assertionMethod": [
                    {
                        "id": "https://social.example/actors/1#main-key",
                        "publicKeyMultibase": "",
                    },
                ],
            },
        });
        let objects = find_objects(&activity);
        let objects_ids: Vec<_> = objects.iter()
            .filter_map(|object| object["id"].as_str())
            .collect();
        assert_eq!(objects_ids, [
            "https://social.example/activities/123",
            "https://social.example/actors/1",
            "https://social.example/actors/1#main-key",
            "https://social.example/actors/1#main-key",
        ]);
    }

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
        assert_eq!(audience.len(), 1);
        assert_eq!(
            audience[0].to_string(),
            "https://social.example/users/1/followers",
        );
    }
}
