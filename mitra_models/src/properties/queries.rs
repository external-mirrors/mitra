use serde::{
    de::DeserializeOwned,
    Serialize,
};
use serde_json::Value as JsonValue;

use crate::database::{
    DatabaseClient,
    DatabaseError,
    DatabaseTypeError,
};

pub async fn set_internal_property(
    db_client: &impl DatabaseClient,
    name: &str,
    value: &impl Serialize,
) -> Result<(), DatabaseError> {
    let value_json = serde_json::to_value(value)
        .map_err(|_| DatabaseTypeError)?;
    db_client.execute(
        "
        INSERT INTO internal_property (property_name, property_value)
        VALUES ($1, $2)
        ON CONFLICT (property_name) DO UPDATE
        SET property_value = $2
        ",
        &[&name, &value_json],
    ).await?;
    Ok(())
}

async fn get_internal_property_json(
    db_client: &impl DatabaseClient,
    name: &str,
) -> Result<Option<JsonValue>, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT property_value
        FROM internal_property
        WHERE property_name = $1
        ",
        &[&name],
    ).await?;
    let maybe_value = maybe_row
        .map(|row| row.try_get("property_value"))
        .transpose()?;
    Ok(maybe_value)
}

pub async fn get_internal_property<T: DeserializeOwned>(
    db_client: &impl DatabaseClient,
    name: &str,
) -> Result<Option<T>, DatabaseError> {
    let maybe_value = match
        get_internal_property_json(db_client, name).await?
    {
        Some(value_json) => {
            let value: T = serde_json::from_value(value_json)
                .map_err(|_| DatabaseTypeError)?;
            Some(value)
        },
        None => None,
    };
    Ok(maybe_value)
}

pub async fn get_internal_properties_json(
    db_client: &impl DatabaseClient,
) -> Result<JsonValue, DatabaseError> {
    let row = db_client.query_one(
        "
        SELECT COALESCE(
            jsonb_object_agg(property_name, property_value),
            '{}'
        ) AS property_map
        FROM internal_property
        ",
        &[],
    ).await?;
    let properties = row.try_get("property_map")?;
    Ok(properties)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_set_internal_property() {
        let db_client = &create_test_database().await;
        let name = "myproperty";
        let value = 100;
        set_internal_property(db_client, name, &value).await.unwrap();
        let db_value: u32 = get_internal_property(db_client, name).await
            .unwrap().unwrap_or_default();
        assert_eq!(db_value, value);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_internal_properties_json() {
        let db_client = &create_test_database().await;
        let properties = get_internal_properties_json(db_client).await.unwrap();
        assert_eq!(properties, json!({}));

        set_internal_property(db_client, "myproperty", &100).await.unwrap();
        let properties = get_internal_properties_json(db_client).await.unwrap();
        assert_eq!(properties, json!({"myproperty": 100}));
    }
}
