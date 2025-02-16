use chrono::{DateTime, SecondsFormat, Utc};
use serde::{
    de::{Error as DeserializerError},
    Deserialize,
    Deserializer,
    Serializer,
};

// https://docs.joinmastodon.org/api/datetime-format/#datetime
pub fn serialize_datetime<S>(
    value: &DateTime<Utc>,
    serializer: S,
) -> Result<S::Ok, S::Error>
    where S: Serializer,
{
    let datetime_str = value.to_rfc3339_opts(SecondsFormat::Millis, true);
    serializer.serialize_str(&datetime_str)
}

pub fn serialize_datetime_opt<S>(
    value: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
    where S: Serializer,
{
    match *value {
        Some(ref dt) => {
            let datetime_str = dt.to_rfc3339_opts(SecondsFormat::Millis, true);
            serializer.serialize_some(&datetime_str)
        },
        None => serializer.serialize_none(),
    }
}

// https://docs.joinmastodon.org/client/intro/#boolean
pub fn deserialize_boolean<'de, D>(
    deserializer: D,
) -> Result<bool, D::Error>
    where D: Deserializer<'de>
{
    let value = String::deserialize(deserializer)?;
    let boolean = match value.to_lowercase().as_str() {
        "true" | "t" | "on" | "1" => true,
        "false" | "f" | "off" | "0" => false,
        _ => return Err(DeserializerError::custom("provided string is not a boolean flag")),
    };
    Ok(boolean)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::json;
    use super::*;

    #[test]
    fn test_deserialize_boolean() {
        #[derive(Deserialize)]
        struct QueryParams {
            #[serde(deserialize_with="deserialize_boolean")]
            test_1: bool,
            #[serde(deserialize_with="deserialize_boolean")]
            test_2: bool,
            #[serde(deserialize_with="deserialize_boolean")]
            test_3: bool,
            #[serde(default, deserialize_with="deserialize_boolean")]
            test_4: bool,
        }

        let value = json!({
            "test_1": "true",
            "test_2": "false",
            "test_3": "1",
        });
        let params: QueryParams = serde_json::from_value(value).unwrap();
        assert_eq!(params.test_1, true);
        assert_eq!(params.test_2, false);
        assert_eq!(params.test_3, true);
        assert_eq!(params.test_4, false);
    }
}
