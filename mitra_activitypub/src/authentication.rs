/// https://codeberg.org/silverpill/feps/src/branch/main/c7d3/fep-c7d3.md
use serde_json::{Value as JsonValue};

use mitra_federation::{
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
