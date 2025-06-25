use anyhow::Error;
use clap::{Parser, ValueEnum};

use mitra_adapters::dynamic_config::{
    set_editable_property,
    EDITABLE_PROPERTIES,
};
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    properties::{
        constants::{
            FEDERATED_TIMELINE_RESTRICTED,
            FILTER_KEYWORDS,
        },
        queries::get_internal_property_json,
    },
};

#[derive(Clone, ValueEnum)]
enum ParameterName {
    /// Make federated timeline visible only to moderators (true of false, default: false)
    #[clap(name = FEDERATED_TIMELINE_RESTRICTED)]
    FederatedTimelineRestricted,
    /// Keywords for reject-keywords filter action (JSON array, example: ["foo", "bar"])
    #[clap(name = FILTER_KEYWORDS)]
    FilterKeywords,
}

impl ParameterName {
    fn as_str(&self) -> &'static str {
        let name_str = match self {
            Self::FederatedTimelineRestricted => FEDERATED_TIMELINE_RESTRICTED,
            Self::FilterKeywords => FILTER_KEYWORDS,
        };
        assert!(EDITABLE_PROPERTIES.contains(&name_str));
        name_str
    }
}

/// Get value of a dynamic configuration parameter
#[derive(Parser)]
pub struct GetConfig {
    name: ParameterName,
}

impl GetConfig {
    pub async fn execute(
        &self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let maybe_value = get_internal_property_json(
            db_client,
            self.name.as_str(),
        ).await?;
        let Some(value) = maybe_value else {
            return Err(Error::msg("value is not set"));
        };
        println!("{value}");
        Ok(())
    }
}

/// Change value of a dynamic configuration parameter
#[derive(Parser)]
pub struct UpdateConfig {
    name: ParameterName,
    value: String,
}

impl UpdateConfig {
    pub async fn execute(
        &self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        set_editable_property(db_client, self.name.as_str(), &self.value).await?;
        println!("configuration updated");
        Ok(())
    }
}
