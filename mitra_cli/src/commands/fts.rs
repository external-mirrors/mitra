use anyhow::Error;
use clap::Parser;

use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    posts::queries::create_fts_index,
};

/// Create an index for full-text search
#[derive(Parser)]
pub struct CreateFtsIndex {
    /// Text search configuration name
    name: String,
}

impl CreateFtsIndex {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        create_fts_index(db_client, &self.name).await?;
        println!("index created");
        Ok(())
    }
}
