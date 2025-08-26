use anyhow::Error;
use clap::Parser;

use mitra_activitypub::{
    builders::delete_person::prepare_delete_person,
};
use mitra_adapters::{
    media::delete_orphaned_media,
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    profiles::{
        helpers::get_profile_by_id_or_acct,
        queries::delete_profile,
    },
    users::queries::get_user_by_id,
};

/// Delete user
#[derive(Parser)]
#[command(visible_alias = "delete-account", alias = "delete-profile")]
pub struct DeleteUser {
    id_or_name: String,
}

impl DeleteUser {
    pub async fn execute(
        &self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let profile = get_profile_by_id_or_acct(
            db_client,
            &self.id_or_name,
        ).await?;
        let mut maybe_delete_person = None;
        if profile.is_local() {
            let user = get_user_by_id(db_client, profile.id).await?;
            let activity =
                prepare_delete_person(db_client, &config.instance(), &user).await?;
            maybe_delete_person = Some(activity);
        };
        let deletion_queue = delete_profile(db_client, profile.id).await?;
        delete_orphaned_media(config, db_client, deletion_queue).await?;
        // Send Delete(Person) activities
        if let Some(activity) = maybe_delete_person {
            activity.save_and_enqueue(db_client).await?;
        };
        println!("user deleted");
        Ok(())
    }
}
