use serde::{
    Deserialize,
    Deserializer,
    de::{Error as DeserializerError},
};
use serde_json::Value;

use crate::errors::ConversionError;

/// Transforms single string or an array value into array of strings
fn parse_string_array(
    value: &Value,
) -> Result<Vec<String>, ConversionError> {
    let result = match value {
        Value::String(string) => vec![string.to_string()],
        Value::Array(array) => array.iter()
            .map(|item| item.to_string()).collect(),
        // Unexpected value type
        _ => return Err(ConversionError),
    };
    Ok(result)
}

pub fn deserialize_string_array<'de, D>(
    deserializer: D,
) -> Result<Vec<String>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<Value> = Option::deserialize(deserializer)?;
    let strings = if let Some(value) = maybe_value {
        parse_string_array(&value).map_err(DeserializerError::custom)?
    } else {
        vec![]
    };
    Ok(strings)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_deserialize_string_array() {
        #[derive(Deserialize)]
        struct TestObject {
            #[serde(deserialize_with = "deserialize_string_array")]
            rel: Vec<String>,
        }

        let value = json!({"rel": "test"});
        let object: TestObject = serde_json::from_value(value).unwrap();
        assert_eq!(object.rel, vec!["test".to_string()]);
    }
}
