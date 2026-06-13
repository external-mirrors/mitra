use anyhow::Error;
use apx_core::url::canonical::CanonicalUri;
use clap::{
    Parser,
    Subcommand,
};

use mitra_activitypub::{
    builders::undo_announce::prepare_undo_announce,
};
use mitra_adapters::{
    media::{delete_files, delete_orphaned_media},
};
use mitra_config::Config;
use mitra_models::{
    accounts::queries::get_user_by_id,
    activitypub::queries::get_object_ids,
    attachments::queries::delete_unused_attachments,
    database::{get_database_client, DatabaseConnectionPool},
    media::queries::{find_orphaned_files, get_local_files},
    posts::queries::{
        delete_post,
        delete_repost,
        find_expired_reposts,
        find_extraneous_posts,
        get_post_by_id,
    },
    profiles::queries::{
        delete_profile,
        find_empty_profiles,
        get_profile_by_id,
    },
};
use mitra_services::media::MediaStorage;
use mitra_utils::datetime::days_before_now;

/// Delete old remote posts
#[derive(Parser)]
pub struct DeleteExtraneousPosts {
    days: u32,
}

impl DeleteExtraneousPosts {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let updated_before = days_before_now(self.days);
        let posts = find_extraneous_posts(db_client, updated_before).await?;
        for post_id in posts {
            let deletion_queue = delete_post(db_client, post_id).await?;
            delete_orphaned_media(config, db_client, deletion_queue).await?;
            println!("post {} deleted", post_id);
        };
        Ok(())
    }
}

/// Delete attachments that don't belong to any post
#[derive(Parser)]
pub struct DeleteUnusedAttachments {
    days: u32,
}

impl DeleteUnusedAttachments {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let created_before = days_before_now(self.days);
        let (deleted_count, deletion_queue) = delete_unused_attachments(
            db_client,
            created_before,
        ).await?;
        delete_orphaned_media(config, db_client, deletion_queue).await?;
        println!("deleted {deleted_count} unused attachments");
        Ok(())
    }
}

/// Delete empty remote profiles
#[derive(Parser)]
pub struct DeleteEmptyProfiles {
    days: u32,
}

impl DeleteEmptyProfiles {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let updated_before = days_before_now(self.days);
        let profiles = find_empty_profiles(db_client, updated_before).await?;
        for profile_id in profiles {
            let profile = get_profile_by_id(db_client, profile_id).await?;
            let deletion_queue = delete_profile(db_client, profile.id).await?;
            delete_orphaned_media(config, db_client, deletion_queue).await?;
            println!("profile deleted: {}", profile.expect_remote_actor_id());
        };
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

/// Delete old reposts made by local users
#[derive(Parser)]
pub struct PruneReposts {
    /// Maximum age (days)
    days: u32,
}

impl PruneReposts {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let updated_before = days_before_now(self.days);
        let reposts = find_expired_reposts(db_client, updated_before).await?;
        for repost in reposts {
            delete_repost(db_client, repost.id).await?;
            let author = get_user_by_id(db_client, repost.author_id).await?;
            let post = get_post_by_id(db_client, repost.repost_of_id).await?;
            prepare_undo_announce(
                db_client,
                &config.instance(),
                &author,
                &post,
                &repost,
            ).await?.save_and_enqueue(db_client).await?;
            println!("deleted repost of post {}", post.id);
        };
        Ok(())
    }
}

/// Validate object IDs stored in database
#[derive(Parser)]
pub struct CheckUris;

impl CheckUris {
    pub async fn execute(
        self,
        _config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let object_ids = get_object_ids(db_client).await?;
        for object_id in object_ids {
            if let Err(error) = CanonicalUri::parse_canonical(&object_id) {
                println!("invalid URI {object_id}: {error}");
            };
        };
        Ok(())
    }
}

/// Manage media
#[derive(Subcommand)]
pub enum MediaCommand {
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
            Self::Local(command) => command.execute(config, db_pool).await,
            Self::DeleteOrphaned(command) => command.execute(config, db_pool).await,
        }
    }
}
