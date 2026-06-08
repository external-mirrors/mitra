use apx_sdk::{
    core::{
        crypto::{
            common::PublicKey,
            eddsa::ed25519_public_key_from_secret_key,
            rsa::RsaPublicKey,
        },
        json_signatures::create::is_object_signed,
    },
    deserialization::object_to_id,
    ownership::is_ownership_ambiguous,
    utils::{
        get_core_type,
        is_key_like,
    },
};
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_models::{
    accounts::types::{PortableUser, User},
    activitypub::queries::get_object,
    database::DatabaseClient,
};
use mitra_validators::errors::ValidationError;

use crate::{
    authority::Authority,
    errors::HandlerError,
    identifiers::{
        canonicalize_id,
        local_actor_id_unified,
    },
    keys::verification_method_to_public_key,
    ownership::{
        get_object_id,
        get_object_id_opt,
        get_owner,
        is_local_origin,
        is_same_id,
    },
    vocabulary::{
        ADD,
        DELETE,
        MOVE,
        REMOVE,
        UNDO,
        UPDATE,
    },
};

pub fn verify_activity_id(
    instance: &Instance,
    activity: &JsonValue,
) -> Result<(), ValidationError> {
    // TODO: replace activity ID to prevent conflicts
    // TODO: replace all local IDs in embedded objects
    let activity_id = get_object_id(activity)?;
    if !is_local_origin(instance, activity_id) {
        return Err(ValidationError("activity ID is not local"));
    };
    Ok(())
}

pub fn verify_activity_actor(
    instance: &Instance,
    account: &User,
    activity: &JsonValue,
) -> Result<(), ValidationError> {
    let activity_actor = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid actor property"))?;
    let authority = Authority::from(instance);
    let expected_actor_id = local_actor_id_unified(
        &authority,
        account.profile.id,
        &account.profile.username,
    );
    if activity_actor != expected_actor_id {
        return Err(ValidationError("actor is not authorized to perform activity"));
    };
    Ok(())
}

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

pub async fn verify_permissions(
    db_client: &impl DatabaseClient,
    object: &JsonValue,
) -> Result<(), HandlerError> {
    let objects = find_objects(object);
    if objects.len() > 20 {
        return Err(ValidationError("too many embedded objects").into());
    };
    for object in objects {
        let (activity, object) = match object["type"].as_str() {
            Some(UPDATE | DELETE | UNDO) => (object, &object["object"]),
            Some(ADD | REMOVE) => (object, &object["target"]),
            // Move: both object and target?
            Some(MOVE) =>
                return Err(ValidationError("Move activity is not allowed").into()),
            // Non-AS activities?
            _ => continue,
        };
        // Actions will appear as same-origin to servers that don't implement FEP-ef61
        let activity_owner = get_owner(activity, get_core_type(activity))?;
        let object_id = object_to_id(object)
            .map_err(|_| ValidationError("unexpected activity structure"))?;
        let canonical_object_id = canonicalize_id(&object_id)?;
        let object = get_object(db_client, &canonical_object_id).await?;
        let object_owner = get_owner(&object, get_core_type(&object))?;
        if !is_same_id(&activity_owner, &object_owner)? {
            return Err(ValidationError(
                "actor is not authorized to perform action"
            ).into());
        };
    };
    Ok(())
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
