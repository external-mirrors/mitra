use anyhow::Error;
use apx_core::url::canonical::CanonicalUrl;
use clap::Parser;

use mitra_activitypub::{
    builders::undo_announce::prepare_undo_announce,
};
use mitra_config::Config;
use mitra_models::{
    activitypub::helpers::get_object_ids,
    database::{get_database_client, DatabaseConnectionPool},
    posts::queries::{
        delete_repost,
        find_expired_reposts,
        get_post_by_id,
    },
    users::queries::get_user_by_id,
};
use mitra_utils::datetime::days_before_now;

/// Delete old reposts made by local users
#[derive(Parser)]
pub struct PruneReposts {
    /// Maximum age (days)
    days: u32,
}

impl PruneReposts {
    pub async fn execute(
        &self,
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

#[derive(Parser)]
pub struct CheckUris;

impl CheckUris {
    pub async fn execute(
        &self,
        _config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let object_ids = get_object_ids(db_client).await?;
        for object_id in object_ids {
            if let Err(error) = CanonicalUrl::parse_canonical(&object_id) {
                println!("invalid URI {object_id}: {error}");
            };
        };
        Ok(())
    }
}
