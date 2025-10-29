use serde_json::{Value as JsonValue};
use thiserror::Error;

use crate::deserialization::parse_into_id_array;

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
}
