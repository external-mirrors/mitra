//! Helper functions for authorization checks.

use serde_json::{Value as JsonValue};
use thiserror::Error;

use crate::{
    deserialization::{object_to_id, parse_into_id_array},
    utils::CoreType,
};

const ATTRIBUTED_TO: &str = "attributedTo";
const ACTOR: &str = "actor";
const OWNER: &str = "owner";
const CONTROLLER: &str = "controller";

/// Error that may occur during the validation of an object
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ObjectError(&'static str);

impl ObjectError {
    /// Returns the error message
    pub fn message(&self) -> &'static str { self.0 }
}

/// Parses the value of an `attributedTo` property
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

fn deny_properties(
    object: &JsonValue,
    properties: &[&str],
) -> Result<(), ObjectError> {
    for property in properties {
        if !object[property].is_null() {
            return Err(ObjectError("ambiguous ownership"));
        };
    };
    Ok(())
}

/// Determines the owner of an object.
///
/// <https://codeberg.org/fediverse/fep/src/branch/main/fep/fe34/fep-fe34.md>
pub fn get_owner(
    object: &JsonValue,
    core_type: CoreType,
) -> Result<String, ObjectError> {
    let maybe_owner = match core_type {
        CoreType::Object | CoreType::Collection => {
            parse_attributed_to(&object[ATTRIBUTED_TO])?
                .ok_or(ObjectError("'attributedTo' property is missing"))
        },
        CoreType::Link => Err(ObjectError("link doesn't have an owner")),
        CoreType::Actor => {
            object["id"].as_str()
                .map(|id| id.to_owned())
                .ok_or(ObjectError("'id' property is missing"))
        },
        CoreType::Activity => {
            object_to_id(&object[ACTOR])
                .map_err(|_| ObjectError("invalid 'actor' property"))
        },
        CoreType::PublicKey => {
            object[OWNER].as_str()
                .map(|id| id.to_owned())
                .ok_or(ObjectError("'owner' property is missing"))
        },
        CoreType::VerificationMethod => {
            object[CONTROLLER].as_str()
                .map(|id| id.to_owned())
                .ok_or(ObjectError("'controller' property is missing"))
        },
    };
    let owner = maybe_owner?;
    // Protection from type confusion attacks.
    // Example: object is duck-typed as actor,
    // but it is a `Note` attributed to a different actor.
    match core_type {
        CoreType::Actor
            | CoreType::Activity
            | CoreType::PublicKey
            | CoreType::VerificationMethod
            =>
        {
            deny_properties(object, &[ATTRIBUTED_TO])?;
        },
        _ => (),
    };
    Ok(owner)
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

    #[test]
    fn test_get_owner_actor() {
        let object = json!({
            "id": "https://social.example/actors/1",
            "type": "Note",
            "inbox": "https://social.example/actors/1/inbox",
            "outbox": "https://social.example/actors/1/outbox",
        });
        let owner = get_owner(&object, CoreType::Actor).unwrap();
        assert_eq!(owner, "https://social.example/actors/1");
    }

    #[test]
    fn test_get_owner_actor_with_attributed_to() {
        let object = json!({
            "id": "https://social.example/actors/1",
            "type": "Note",
            "inbox": "https://social.example/actors/1/inbox",
            "outbox": "https://social.example/actors/1/outbox",
            "attributedTo": "https://social.example/actors/2",
        });
        let error = get_owner(&object, CoreType::Actor).err().unwrap();
        assert_eq!(error.message(), "ambiguous ownership");
    }
}
