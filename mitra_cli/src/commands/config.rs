use anyhow::Error;
use clap::Parser;

use mitra_adapters::dynamic_config::{
    set_editable_property,
    EDITABLE_PROPERTIES,
};
use mitra_models::database::DatabaseClient;

/// Change value of a dynamic configuration parameter
///
/// - federated_timeline_restricted (true of false, default: false): make federated timeline visible only to moderators.
#[derive(Parser)]
pub struct UpdateConfig {
    #[arg(value_parser = EDITABLE_PROPERTIES)]
    name: String,
    value: String,
}

impl UpdateConfig {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        set_editable_property(db_client, &self.name, &self.value).await?;
        println!("configuration updated");
        Ok(())
    }
}
