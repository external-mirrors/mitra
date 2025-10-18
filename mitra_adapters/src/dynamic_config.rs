use serde_json::{Value as JsonValue};

use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    properties::constants::{
        FEDERATED_TIMELINE_RESTRICTED,
        FILTER_KEYWORDS,
    },
    properties::queries::{
        get_internal_property,
    },
};
use mitra_validators::errors::ValidationError;

// Dynamic configuration parameters
pub const EDITABLE_PROPERTIES: [&str; 2] = [
    // Make federated timeline visible only to moderators
    FEDERATED_TIMELINE_RESTRICTED,
    // Keywords for `reject-keywords` filter action
    FILTER_KEYWORDS,
];

pub fn validate_editable_parameter(
    name: &str,
    value: &JsonValue,
) -> Result<(), ValidationError> {
    let value = value.clone();
    match name {
        FEDERATED_TIMELINE_RESTRICTED => {
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

pub struct DynamicConfig {
    pub federated_timeline_restricted: bool,
}

pub async fn get_dynamic_config(
    db_client: &impl DatabaseClient,
) -> Result<DynamicConfig, DatabaseError> {
    let federated_timeline_restricted: bool =
        get_internal_property(db_client, FEDERATED_TIMELINE_RESTRICTED)
            .await?
            .unwrap_or(false);
    let config = DynamicConfig { federated_timeline_restricted };
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
