// https://codeberg.org/fediverse/fep/src/branch/main/fep/fe34/fep-fe34.md
use serde_json::{Value as JsonValue};

use apx_sdk::{
    deserialization::{object_to_id, parse_into_id_array},
    url::{is_same_origin as apx_is_same_origin},
};
use mitra_validators::errors::ValidationError;

pub fn get_object_id(object: &JsonValue) -> Result<&str, ValidationError> {
    object["id"].as_str()
        .ok_or(ValidationError("'id' property is missing"))
}

pub fn is_same_origin(id_1: &str, id_2: &str) -> Result<bool, ValidationError> {
    apx_is_same_origin(id_1, id_2)
        .map_err(|error| ValidationError(error.0))
}

pub fn verify_activity_owner(
    activity: &JsonValue,
) -> Result<(), ValidationError> {
    let activity_id = get_object_id(activity)?;
    let owner_id = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid 'actor' property"))?;
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
        .to_string();
    Ok(owner_id)
}

pub fn verify_object_owner(
    object: &JsonValue,
) -> Result<(), ValidationError> {
    let object_id = get_object_id(object)?;
    let owner_id = parse_attributed_to(&object["attributedTo"])?;
    let is_valid = is_same_origin(object_id, &owner_id)?;
    if !is_valid {
        return Err(ValidationError("owner has different origin"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_verify_object_owner() {
        let object = json!({
            "@context": ["https://www.w3.org/ns/activitystreams"],
            "attributedTo": "https://social.example/actor",
            "id": "https://social.example/objects/123",
            "type":"Note",
        });
        let result = verify_object_owner(&object);
        assert_eq!(result.is_ok(), true);
    }
}
