use anyhow::Error;
use clap::{Parser, Subcommand};

use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    posts::queries::{
        create_fts_index_unsafe,
        delete_fts_index_unsafe,
        get_fts_indexes,
    },
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
        create_fts_index_unsafe(db_client, &self.name).await?;
        println!("index created");
        Ok(())
    }
}

/// Delete a full-text search index
#[derive(Parser)]
pub struct DeleteFtsIndex {
    /// Text search configuration name
    name: String,
}

impl DeleteFtsIndex {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        delete_fts_index_unsafe(db_client, &self.name).await?;
        println!("index deleted");
        Ok(())
    }
}

/// List full-text search indices
#[derive(Parser)]
pub struct ListFtsIndexes;

impl ListFtsIndexes {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let indexes = get_fts_indexes(db_client).await?;
        for index in indexes {
            println!("{index}");
        };
        Ok(())
    }
}

/// Manage full-text search indices
#[derive(Subcommand)]
pub enum FtsCommand {
    Create(CreateFtsIndex),
    #[command(visible_alias = "remove")]
    Delete(DeleteFtsIndex),
    List(ListFtsIndexes),
}

impl FtsCommand {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        match self {
            Self::Create(command) => command.execute(db_pool).await,
            Self::Delete(command) => command.execute(db_pool).await,
            Self::List(command) => command.execute(db_pool).await,
        }
    }
}
