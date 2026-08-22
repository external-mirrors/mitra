use anyhow::Error;
use apx_sdk::fetch::fetch_media;
use clap::{
    Parser,
    Subcommand,
};

use mitra_activitypub::agent::build_federation_agent;
use mitra_adapters::media::delete_files;
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    media::queries::{find_orphaned_files, get_local_files},
};
use mitra_services::media::MediaStorage;

/// Fetch file and save it to the media directory
#[derive(Parser)]
pub struct FetchMedia {
    url: String,
}

impl FetchMedia {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let agent = build_federation_agent(&config.instance(), None);
        let media_storage = MediaStorage::new(config);
        let (file_data, media_type) = fetch_media(
            &agent,
            &self.url,
            &config.limits.media.supported_media_types(),
            config.limits.media.file_size_limit,
        ).await?;
        let file_info = media_storage.save_file(file_data, &media_type)?;
        println!("file saved: {}", file_info.file_name);
        Ok(())
    }
}

/// List files uploaded by local users
#[derive(Parser)]
pub struct ListLocalFiles;

impl ListLocalFiles {
    pub async fn execute(
        self,
        _config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let filenames = get_local_files(db_client).await?;
        for file_name in filenames {
            println!("{file_name}");
        };
        Ok(())
    }
}

/// Find and delete orphaned files
#[derive(Parser)]
pub struct DeleteOrphanedFiles {
    /// List found files, but don't delete them
    #[arg(long)]
    dry_run: bool,
}

impl DeleteOrphanedFiles {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let media_storage = MediaStorage::new(config);
        let files = media_storage.list_files()?;
        let orphaned = find_orphaned_files(db_client, files).await?;
        if orphaned.is_empty() {
            println!("no orphaned files found");
            return Ok(());
        };
        if self.dry_run {
            for file_name in orphaned {
                println!("orphaned file: {file_name}");
            };
        } else {
            delete_files(&media_storage, &orphaned);
            println!("orphaned files deleted: {}", orphaned.len());
        };
        Ok(())
    }
}

/// Manage media
#[derive(Subcommand)]
pub enum MediaCommand {
    Fetch(FetchMedia),
    Local(ListLocalFiles),
    DeleteOrphaned(DeleteOrphanedFiles),
}

impl MediaCommand {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        match self {
            Self::Fetch(command) => command.execute(config).await,
            Self::Local(command) => command.execute(config, db_pool).await,
            Self::DeleteOrphaned(command) => command.execute(config, db_pool).await,
        }
    }
}
