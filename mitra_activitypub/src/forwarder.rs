use apx_sdk::{
    core::{
        crypto::{
            common::PublicKey,
            eddsa::ed25519_public_key_from_secret_key,
            rsa::RsaPublicKey,
        },
        json_signatures::create::is_object_signed,
        url::canonical::CanonicalUri,
    },
    ownership::is_ownership_ambiguous,
    utils::{
        get_core_type,
        is_key_like,
    },
};
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_models::{
    activitypub::queries::expand_collections,
    database::{DatabaseClient, DatabaseError},
    profiles::{
        queries::get_remote_profiles_by_actor_ids,
        types::DbActorProfile,
    },
    users::types::PortableUser,
};
use mitra_validators::errors::ValidationError;

use crate::{
    keys::verification_method_to_public_key,
    ownership::{
        get_object_id_opt,
        get_owner,
        is_local_origin,
        is_same_id,
    },
};

fn find_objects(object: &JsonValue) -> Vec<&JsonValue> {
    let mut objects = vec![];
    match object {
        JsonValue::Object(map) => {
            objects.push(object);
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

pub fn verify_public_keys(
    instance: &Instance,
    maybe_account: Option<&PortableUser>,
    object: &JsonValue,
) -> Result<(), ValidationError> {
    let objects = find_objects(object);
    for object in objects {
        // WARNING: this is not reliable if JSON-LD is used
        // https://codeberg.org/fediverse/fep/src/commit/8862845a2b71a32e254932757ef7696b6714739d/fep/2277/fep-2277.md#json-ld
        if !is_key_like(object) {
            continue;
        };
        let Some(object_id) = get_object_id_opt(object) else {
            // Skip anonymous objects
            continue;
        };
        if !is_local_origin(instance, object_id) {
            continue;
        };
        let public_key = verification_method_to_public_key(object.clone())?;
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

pub fn verify_embedded_ownership(
    object: &JsonValue,
) -> Result<(), ValidationError> {
    let root_owner = get_owner(object, get_core_type(object))?;
    let objects = find_objects(object);
    for object in objects {
        if get_object_id_opt(object).is_none() {
            // Skip anonymous objects
            continue;
        };
        if is_object_signed(object) {
            // Skip signed objects
            continue;
        };
        // Embedded object must have the same owner
        let object_class = get_core_type(object);
        let object_owner = get_owner(object, object_class)?;
        if is_ownership_ambiguous(object, object_class) {
            return Err(ValidationError("ambiguous ownership"));
        };
        if !is_same_id(&object_owner, &root_owner)? {
            return Err(ValidationError("embedded object has different owner"))
        };
    };
    Ok(())
}

/// Returns remote recipients of the activity
pub async fn get_activity_recipients(
    db_client: &impl DatabaseClient,
    audience: &[CanonicalUri],
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let expanded_audience = expand_collections(
        db_client,
        audience,
    ).await?;
    let recipients = get_remote_profiles_by_actor_ids(
        db_client,
        &expanded_audience,
    ).await?;
    Ok(recipients)
}

pub enum EndpointType {
    Inbox,
    Outbox,
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
    fn test_verify_public_keys_no_keys() {
        let instance = Instance::for_test("social.example");
        let activity = json!({
            "id": "https://social.example/activities/123",
            "type": "Create",
            "actor": "https://social.example/actors/1",
            "object": {
                "id": "https://social.example/notes/1",
                "type": "Note",
                "attributedTo": "https://social.example/actors/1",
            },
        });
        let result = verify_public_keys(&instance, None, &activity);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_verify_public_keys_public_key_pem() {
        let instance = Instance::for_test("social.example");
        let activity = json!({
            "id": "https://social.example/activities/123",
            "type": "Create",
            "actor": "https://social.example/actors/1",
            "object": {
                "id": "https://social.example/notes/1",
                "type": "Note",
                "attributedTo": "https://social.example/actors/1",
                "attachment": {
                    "id": "https://social.example/actors/1/key",
                    "owner": "https://social.example/actors/1",
                    "publicKeyPem": "-----BEGIN PUBLIC KEY-----\nMFwwDQYJKoZIhvcNAQEBBQADSwAwSAJBAOIh58ZQbo45MuZvv1nMWAzTzN9oghNC\nbxJkFEFD1Y49LEeNHMk6GrPByUz8kn4y8Hf6brb+DVm7ZW4cdhOx1TsCAwEAAQ==\n-----END PUBLIC KEY-----",
                },
            },
        });
        let result = verify_public_keys(&instance, None, &activity);
        assert_eq!(result.err().unwrap().0, "unexpected public key");
    }

    #[test]
    fn test_verify_embedded_ownership() {
        let activity = json!({
            "id": "https://social.example/activities/123",
            "type": "Create",
            "actor": "https://social.example/actors/1",
            "object": {
                "id": "https://social.example/notes/1",
                "type": "Note",
                "attributedTo": "https://social.example/actors/1",
            },
        });
        let result = verify_embedded_ownership(&activity);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_verify_embedded_ownership_error() {
        let activity = json!({
            "id": "https://social.example/activities/123",
            "type": "Create",
            "actor": "https://social.example/actors/1",
            "object": {
                "id": "https://social.example/notes/1",
                "type": "Note",
                "attributedTo": "https://social.example/actors/1",
                "replies": {
                    "type": "Collection",
                    "items": [
                        {
                            "type": "Note",
                            "id": "https://social.example/notes/2",
                            // Different owner!
                            "attributedTo": "https://social.example/actors/2",
                        },
                    ],
                },
            },
        });
        let result = verify_embedded_ownership(&activity);
        assert_eq!(result.err().unwrap().0, "embedded object has different owner");
    }
}
