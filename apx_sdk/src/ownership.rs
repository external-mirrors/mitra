use serde_json::{Value as JsonValue};
use thiserror::Error;

use crate::{
    deserialization::{object_to_id, parse_into_id_array},
    utils::CoreType,
};

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ObjectError(&'static str);

impl ObjectError {
    pub fn message(&self) -> &'static str { self.0 }
}

pub fn parse_attributed_to(
    value: &JsonValue,
) -> Result<Option<String>, ObjectError> {
    let maybe_attributed_to = parse_into_id_array(value)
        .map_err(|_| ObjectError("invalid 'attributedTo' property"))?
        // Take first value if there are more than one
        .first()
        .cloned();
    Ok(maybe_attributed_to)
}

pub fn get_owner(
    object: &JsonValue,
    core_type: CoreType,
) -> Result<String, ObjectError> {
    match core_type {
        CoreType::Object | CoreType::Collection => {
            parse_attributed_to(&object["attributedTo"])?
                .ok_or(ObjectError("'attributedTo' property is missing"))
        },
        CoreType::Link => Err(ObjectError("link doesn't have an owner")),
        CoreType::Actor => {
            object["id"].as_str()
                .map(|id| id.to_owned())
                .ok_or(ObjectError("'id' property is missing"))
        },
        CoreType::Activity => {
            object_to_id(&object["actor"])
                .map_err(|_| ObjectError("invalid 'actor' property"))
        },
        CoreType::VerificationMethod => {
            object["controller"].as_str()
                .or(object["owner"].as_str())
                .map(|id| id.to_owned())
                .ok_or(ObjectError("verification method without owner"))
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_parse_attributed_to_array() {
        let object = json!({
            "id": "https://social.example/objects/123",
            "type": "Note",
            "attributedTo": [
                "https://social.example/actors/1",
                "https://social.example/actors/2",
            ],
        });
        let attributed_to = parse_attributed_to(&object["attributedTo"])
            .unwrap().unwrap();
        assert_eq!(attributed_to, "https://social.example/actors/1");
    }

    #[test]
    fn test_get_owner_object() {
        let object = json!({
            "id": "https://social.example/objects/123",
            "type": "Note",
            "attributedTo": "https://social.example/actors/1",
        });
        let owner = get_owner(&object, CoreType::Object).unwrap();
        assert_eq!(owner, "https://social.example/actors/1");

        let error = get_owner(&object, CoreType::Activity).err().unwrap();
        assert_eq!(error.message(), "invalid 'actor' property");
    }
}
