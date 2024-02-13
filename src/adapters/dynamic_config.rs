use mitra_models::{
    database::{DatabaseClient, DatabaseError, DatabaseTypeError},
    properties::constants::FEDERATED_TIMELINE_RESTRICTED,
    properties::queries::{
        get_internal_property,
        set_internal_property,
    },
};

// Dynamic configuration parameters
pub const EDITABLE_PROPERTIES: [&str; 1] = [
    // Make federated timeline visible only to moderators
    FEDERATED_TIMELINE_RESTRICTED,
];

pub async fn set_editable_property(
    db_client: &impl DatabaseClient,
    name: &str,
    value: &str,
) -> Result<(), DatabaseError> {
    let value_json = match name {
        FEDERATED_TIMELINE_RESTRICTED => {
            // TODO: return validation error
            let value_typed: bool = serde_json::from_str(value)
                .map_err(|_| DatabaseTypeError)?;
            serde_json::json!(value_typed)
        },
        _ => return Err(DatabaseTypeError.into()),
    };
    // TODO: avoid converting to Value twice
    set_internal_property(db_client, name, &value_json).await
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
