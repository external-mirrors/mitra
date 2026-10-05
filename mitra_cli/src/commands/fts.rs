use anyhow::Error;
use clap::{Parser, Subcommand};

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

/// Manage full-text search indices
#[derive(Subcommand)]
pub enum FtsCommand {
    Create(CreateFtsIndex),
}

impl FtsCommand {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        match self {
            Self::Create(command) => command.execute(db_pool).await,
        }
    }
}
