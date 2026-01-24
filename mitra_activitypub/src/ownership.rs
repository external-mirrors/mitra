// https://codeberg.org/fediverse/fep/src/branch/main/fep/fe34/fep-fe34.md
use apx_sdk::{
    core::url::{
        canonical::{
            is_same_origin as apx_is_same_origin,
            is_same_uri,
        },
        http_uri::HttpUri,
    },
    ownership::{
        get_owner as apx_get_owner,
        is_ownership_ambiguous,
        parse_attributed_to as apx_parse_attributed_to,
    },
    utils::CoreType,
};
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_validators::errors::ValidationError;

pub fn get_object_id_opt(object: &JsonValue) -> Option<&str> {
    object["id"].as_str()
}

pub fn get_object_id(object: &JsonValue) -> Result<&str, ValidationError> {
    get_object_id_opt(object)
        .ok_or(ValidationError("'id' property is missing"))
}

pub fn is_same_origin(id_1: &str, id_2: &str) -> Result<bool, ValidationError> {
    apx_is_same_origin(id_1, id_2)
        .map_err(|error| ValidationError(error.0))
}

pub fn get_owner(
    object: &JsonValue,
    core_type: CoreType,
) -> Result<String, ValidationError> {
    let owner = apx_get_owner(object, core_type)
        .map_err(|error| ValidationError(error.message()))?;
    if is_ownership_ambiguous(object, core_type) {
        log::warn!("ambiguous ownership ({core_type:?})");
    };
    Ok(owner)
}

pub fn is_same_id(id_1: &str, id_2: &str) -> Result<bool, ValidationError> {
    is_same_uri(id_1, id_2)
        .map_err(|error| ValidationError(error.0))
}

pub fn verify_activity_owner(
    activity: &JsonValue,
) -> Result<(), ValidationError> {
    let activity_id = get_object_id(activity)?;
    let owner_id = get_owner(activity, CoreType::Activity)?;
    let is_valid = is_same_origin(activity_id, &owner_id)?;
    if !is_valid {
        return Err(ValidationError("owner has different origin"));
    };
    Ok(())
}

pub fn parse_attributed_to(
    attributed_to: &JsonValue,
) -> Result<String, ValidationError> {
    let owner_id = apx_parse_attributed_to(attributed_to)
        .map_err(|error| ValidationError(error.message()))?
        .ok_or(ValidationError("missing 'attributedTo' property"))?
        .clone();
    Ok(owner_id)
}

pub fn verify_object_owner(
    object: &JsonValue,
) -> Result<(), ValidationError> {
    let object_id = get_object_id(object)?;
    let owner_id = get_owner(object, CoreType::Object)?;
    let is_valid = is_same_origin(object_id, &owner_id)?;
    if !is_valid {
        return Err(ValidationError("owner has different origin"));
    };
    Ok(())
}

// Local objects must be rejected when they enter the system via:
// 1. Fetcher
// 2. Inboxes (regular and portable)
// 3. Embedded signed objects
// Local activities are only permitted in portable outboxes,
// where they should be validated.
#[cfg(not(feature = "mini"))]
pub fn is_local_origin(
    instance: &Instance,
    object_id: &str,
) -> bool {
    if let Ok(http_object_id) = HttpUri::parse(object_id) {
        if http_object_id.hostname() == instance.uri().hostname() {
            return true;
        };
    };
    false
}

#[cfg(feature = "mini")]
pub fn is_local_origin(
    _instance: &Instance,
    _object_id: &str,
) -> bool {
    // The check only makes sense on the server
    false
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_verify_activity_owner() {
        let activity = json!({
            "@context": ["https://www.w3.org/ns/activitystreams"],
            "id": "https://social.example/activities/123",
            "type": "Announce",
            "actor": "https://social.example/actor",
            "object": "https://social.example/objects/876",
        });
        let result = verify_activity_owner(&activity);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_verify_object_owner() {
        let object = json!({
            "@context": ["https://www.w3.org/ns/activitystreams"],
            "attributedTo": "https://social.example/actor",
            "id": "https://social.example/objects/123",
            "type": "Note",
        });
        let result = verify_object_owner(&object);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_is_local_origin() {
        let instance = Instance::for_test("https://local.example");
        let object_id = "https://local.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1";
        assert_eq!(is_local_origin(&instance, object_id), true);
        let object_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1";
        assert_eq!(is_local_origin(&instance, object_id), false);
    }
}
