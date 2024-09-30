/// https://codeberg.org/silverpill/feps/src/branch/main/c7d3/fep-c7d3.md
use serde_json::{Value as JsonValue};

use apx_sdk::{
    deserialization::{get_object_id, parse_into_id_array},
    url::is_same_origin,
};
use mitra_validators::errors::ValidationError;

pub fn verify_activity_owner(
    activity: &JsonValue,
) -> Result<(), ValidationError> {
    let activity_id = activity["id"].as_str()
        .ok_or(ValidationError("'id' property is missing"))?;
    let owner_id = get_object_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid 'actor' property"))?;
    let is_valid = is_same_origin(activity_id, &owner_id)
        .map_err(|error| ValidationError(error.0))?;
    if !is_valid {
        return Err(ValidationError("owner has different origin"));
    };
    Ok(())
}

// Can be used for verifying FEP-1b12 activities
pub fn is_embedded_activity_trusted(
    activity: &JsonValue,
) -> Result<bool, ValidationError> {
    let owner_id = get_object_id(&activity["actor"])
        .map_err(|_| ValidationError("invalid 'actor' property"))?;
    let embedded_owner_id = activity["object"]["actor"].as_str()
        .ok_or(ValidationError("'object.actor' property is missing"))?;
    let is_trusted = is_same_origin(&owner_id, embedded_owner_id)
        .map_err(|error| ValidationError(error.0))?;
    Ok(is_trusted)
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
    let object_id = object["id"].as_str()
        .ok_or(ValidationError("'id' property is missing"))?;
    let owner_id = parse_attributed_to(&object["attributedTo"])?;
    let is_valid = is_same_origin(object_id, &owner_id)
        .map_err(|error| ValidationError(error.0))?;
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
    fn test_is_embedded_activity_trusted() {
        let activity = json!({
            "@context": ["https://join-lemmy.org/context.json","https://www.w3.org/ns/activitystreams"],
            "actor": "https://lemmy.example/c/test",
            "cc": ["https://lemmy.example/c/test/followers"],
            "id": "https://lemmy.example/activities/announce/like/7876c523-64c1-4c95-be5f-66b1f79f3186",
            "object":{
                "@context":["https://join-lemmy.org/context.json","https://www.w3.org/ns/activitystreams"],
                "actor":"https://lemmy-user.example/u/test",
                "audience":"https://lemmy.example/c/test",
                "id": "https://lemmy-user.example/activities/like/986c14db-1a8c-4ab6-b67d-14423e52c169",
                "object":"https://lemmy.example/post/18537913",
                "type": "Like",
            },
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
            "type":"Announce",
        });
        let is_trusted = is_embedded_activity_trusted(&activity).unwrap();
        assert_eq!(is_trusted, false);
    }
}
