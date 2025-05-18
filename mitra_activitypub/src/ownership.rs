// https://codeberg.org/fediverse/fep/src/branch/main/fep/fe34/fep-fe34.md
use serde_json::{Value as JsonValue};

use apx_sdk::{
    core::{
        http_url::HttpUrl,
        url::canonical::{is_same_origin as apx_is_same_origin},
    },
    deserialization::{object_to_id, parse_into_id_array},
    utils::CoreType,
};
use mitra_config::Instance;
use mitra_validators::errors::ValidationError;

pub fn get_object_id(object: &JsonValue) -> Result<&str, ValidationError> {
    object["id"].as_str()
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
    match core_type {
        CoreType::Object | CoreType::Collection => {
            parse_attributed_to(&object["attributedTo"])
        },
        CoreType::Link => Err(ValidationError("link doesn't have an owner")),
        CoreType::Actor => get_object_id(object).map(|id| id.to_owned()),
        CoreType::Activity => {
            object_to_id(&object["actor"])
                .map_err(|_| ValidationError("invalid 'actor' property"))
        },
        CoreType::VerificationMethod => {
            object["controller"].as_str()
                .or(object["owner"].as_str())
                .map(|id| id.to_owned())
                .ok_or(ValidationError("verification method without owner"))
        },
    }
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
    let owner_id = parse_into_id_array(attributed_to)
        .map_err(|_| ValidationError("invalid 'attributedTo' property"))?
        .first()
        .ok_or(ValidationError("invalid 'attributedTo' property"))?
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
pub fn is_local_origin(
    instance: &Instance,
    object_id: &str,
) -> bool {
    if let Ok(http_object_id) = HttpUrl::parse(object_id) {
        if http_object_id.hostname() == instance.url_ref().hostname() {
            return true;
        };
    };
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
