use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use mitra_models::{
    database::{DatabaseClient, DatabaseError, DatabaseTypeError},
    properties::constants::{
        FEDERATED_TIMELINE_RESTRICTED,
        FILTER_BLOCKLIST_PUBLIC,
        FILTER_KEYWORDS,
    },
    properties::queries::{
        get_internal_properties_json,
    },
};
use mitra_validators::errors::ValidationError;

// Dynamic configuration parameters
pub const EDITABLE_PROPERTIES: [&str; 3] = [
    FEDERATED_TIMELINE_RESTRICTED,
    FILTER_BLOCKLIST_PUBLIC,
    FILTER_KEYWORDS,
];

pub fn validate_editable_parameter(
    name: &str,
    value: &JsonValue,
) -> Result<(), ValidationError> {
    let value = value.clone();
    match name {
        FEDERATED_TIMELINE_RESTRICTED
            | FILTER_BLOCKLIST_PUBLIC =>
        {
            let _: bool = serde_json::from_value(value)
                .map_err(|_| ValidationError("invalid value type"))?;
        },
        FILTER_KEYWORDS => {
            let _: Vec<String> = serde_json::from_value(value)
                .map_err(|_| ValidationError("invalid value type"))?;
        },
        _ => return Err(ValidationError("invalid parameter name")),
    };
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct DynamicConfig {
    pub federated_timeline_restricted: bool,
    pub filter_blocklist_public: bool,
    pub filter_keywords: Vec<String>,
}

#[allow(clippy::derivable_impls)]
impl Default for DynamicConfig {
    fn default() -> Self {
        Self {
            federated_timeline_restricted: false,
            filter_blocklist_public: false,
            filter_keywords: vec![],
        }
    }
}

pub async fn get_dynamic_config(
    db_client: &impl DatabaseClient,
) -> Result<DynamicConfig, DatabaseError> {
    let config_json = get_internal_properties_json(db_client).await?;
    let config = serde_json::from_value(config_json)
        .map_err(|_| DatabaseTypeError)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        properties::queries::set_internal_property,
    };
    use super::*;

    #[test]
    fn test_validate_editable_parameter_unknown_property() {
        let name = "test";
        let value = json!(false);
        let error = validate_editable_parameter(name, &value).err().unwrap();
        assert_eq!(error.to_string(), "invalid parameter name");
    }

    #[test]
    fn test_dynamic_config_keys() {
        let config = DynamicConfig::default();
        let config_json = serde_json::to_value(config).unwrap();
        let keys: Vec<_> = config_json.as_object().unwrap().keys().collect();
        assert_eq!(keys, EDITABLE_PROPERTIES);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_dynamic_config() {
        let db_client = &create_test_database().await;
        let config = get_dynamic_config(db_client).await.unwrap();
        assert_eq!(config.federated_timeline_restricted, false);
        set_internal_property(
            db_client,
            FEDERATED_TIMELINE_RESTRICTED,
            &json!(true),
        ).await.unwrap();
        let config = get_dynamic_config(db_client).await.unwrap();
        assert_eq!(config.federated_timeline_restricted, true);
    }
}
