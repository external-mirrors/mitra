use serde::{
    Deserialize,
    Deserializer,
    de::{
        DeserializeOwned,
        Error as DeserializerError,
    },
};
use serde_json::Value;

use mitra_validators::errors::ValidationError;

/// Parses object json value and returns its ID as string
pub fn get_object_id(
    object: &Value,
) -> Result<String, ValidationError> {
    let object_id = match object {
        Value::String(string) => string.to_owned(),
        Value::Object(_) => {
            object["id"].as_str()
                .ok_or(ValidationError("missing 'id' property"))?
                .to_owned()
        },
        _ => return Err(ValidationError("unexpected value type")),
    };
    Ok(object_id)
}

pub fn deserialize_into_object_id<'de, D>(
    deserializer: D,
) -> Result<String, D::Error>
    where D: Deserializer<'de>
{
    let value = Value::deserialize(deserializer)?;
    let object_id = get_object_id(&value)
        .map_err(DeserializerError::custom)?;
    Ok(object_id)
}

/// Transforms single string or an array value into array of strings
fn parse_string_array(
    value: &Value,
) -> Result<Vec<String>, ValidationError> {
    let result = match value {
        Value::String(string) => vec![string.to_string()],
        Value::Array(array) => {
            let mut items = vec![];
            for value in array {
                let string = value.as_str()
                    .ok_or(ValidationError("unexpected array item type"))?
                    .to_string();
                items.push(string);
            };
            items
        },
        _ => return Err(ValidationError("unexpected value type")),
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

/// Transforms arbitrary property value into array of object IDs
pub fn parse_into_id_array(
    value: &Value,
) -> Result<Vec<String>, ValidationError> {
    let result = match value {
        Value::Null => vec![],
        Value::String(_) | Value::Object(_) => {
            let object_id = get_object_id(value)?;
            vec![object_id]
        },
        Value::Array(array) => {
            let mut results = vec![];
            for value in array {
                let object_id = get_object_id(value)?;
                results.push(object_id);
            };
            results
        },
        // Unexpected value type
        _ => return Err(ValidationError("unexpected value type")),
    };
    Ok(result)
}

/// Parses link object and returns its "href"
fn get_link_href(link: &Value) -> Result<String, ValidationError> {
    let href = match link {
        Value::String(string) => string.to_owned(),
        Value::Object(_) => {
            link["href"].as_str()
                .ok_or(ValidationError("missing href property"))?
                .to_string()
        },
        _ => return Err(ValidationError("unexpected value type")),
    };
    Ok(href)
}

/// Transforms arbitrary property value into array of links
pub fn parse_into_href_array(
    value: &Value,
) -> Result<Vec<String>, ValidationError> {
    let result = match value {
        Value::String(_) | Value::Object(_) => {
            let object_id = get_link_href(value)?;
            vec![object_id]
        },
        Value::Array(array) => {
            let mut results = vec![];
            for value in array {
                let object_id = get_link_href(value)?;
                results.push(object_id);
            };
            results
        },
        // Unexpected value type
        _ => return Err(ValidationError("unexpected value type")),
    };
    Ok(result)
}

/// Transforms arbitrary property value into array of structs
pub fn parse_into_array<T: DeserializeOwned>(
    value: &Value,
) -> Result<Vec<T>, ValidationError> {
    let objects = match value {
        Value::Array(array) => array.to_vec(),
        Value::Object(_) => vec![value.clone()],
        // Unexpected value type
        _ => return Err(ValidationError("unexpected value type")),
    };
    let mut items = vec![];
    for object in objects {
        let item: T = serde_json::from_value(object)
            .map_err(|_| ValidationError("invalid array item"))?;
        items.push(item);
    };
    Ok(items)
}

pub fn deserialize_object_array<'de, D, T>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
    where D: Deserializer<'de>, T: DeserializeOwned
{
    let maybe_value: Option<Value> = Option::deserialize(deserializer)?;
    let objects = if let Some(value) = maybe_value {
        parse_into_array(&value).map_err(DeserializerError::custom)?
    } else {
        vec![]
    };
    Ok(objects)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_get_object_id_from_string() {
        let value = json!("test_id");
        assert_eq!(get_object_id(&value).unwrap(), "test_id");
    }

    #[test]
    fn test_get_object_id_from_object() {
        let value = json!({"id": "test_id", "type": "Note"});
        assert_eq!(get_object_id(&value).unwrap(), "test_id");
    }

    #[test]
    fn test_get_object_id_from_array() {
        let value = json!(["test_id"]);
        assert_eq!(
            get_object_id(&value).err().unwrap().to_string(),
            "unexpected value type",
        );
    }

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

        let value = json!({"rel": ["a", "b"]});
        let object: TestObject = serde_json::from_value(value).unwrap();
        assert_eq!(
            object.rel,
            vec!["a".to_string(), "b".to_string()],
        );
    }

    #[test]
    fn test_parse_into_id_array_with_string() {
        let value = json!("test");
        assert_eq!(
            parse_into_id_array(&value).unwrap(),
            vec!["test".to_string()],
        );
    }

    #[test]
    fn test_parse_into_id_array_with_array() {
        let value = json!(["test1", "test2"]);
        assert_eq!(
            parse_into_id_array(&value).unwrap(),
            vec!["test1".to_string(), "test2".to_string()],
        );
    }

    #[test]
    fn test_parse_into_id_array_with_array_of_objects() {
        let value = json!([{"id": "test1"}, {"id": "test2"}]);
        assert_eq!(
            parse_into_id_array(&value).unwrap(),
            vec!["test1".to_string(), "test2".to_string()],
        );
    }

    #[test]
    fn test_parse_into_id_array_with_empty() {
        let object = json!({"key": 1});
        let value = &object["test"];
        assert_eq!(
            parse_into_id_array(value).unwrap().is_empty(),
            true,
        );
    }

    #[test]
    fn test_get_link_href() {
        let link = json!({"name": "test", "href": "https://test.example"});
        assert_eq!(
            get_link_href(&link).unwrap(),
            "https://test.example",
        );
    }

    #[test]
    fn test_parse_into_href_array() {
        let value = json!([
            "https://test1.example",
            "https://test2.example",
        ]);
        assert_eq!(
            parse_into_href_array(&value).unwrap(),
            vec![
                "https://test1.example".to_string(),
                "https://test2.example".to_string(),
            ],
        );
    }

    #[test]
    fn test_parse_into_array_tag_list() {
        let value = json!({"type": "Mention"});
        let value_list: Vec<Value> = parse_into_array(&value).unwrap();
        assert_eq!(value_list, vec![value]);
    }
}
