use anyhow::Error;
use clap::Parser;

use mitra_activitypub::{
    adapters::users::delete_user,
};
use mitra_config::Config;
use mitra_models::{
    accounts::queries::get_user_by_id,
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    profiles::{
        helpers::get_profile_by_id_or_acct,
        queries::{
            delete_profile,
            find_unreachable,
        },
    },
};
use mitra_utils::datetime::days_before_now;

/// List unreachable actors
#[derive(Parser)]
pub struct ListUnreachableActors {
    days: u32,
}

impl ListUnreachableActors {
    pub async fn execute(
        self,
        _config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let unreachable_since = days_before_now(self.days);
        let profiles = find_unreachable(db_client, unreachable_since).await?;
        println!(
            "{0: <60} | {1: <35} | {2: <35}",
            "ID", "unreachable since", "updated at",
        );
        for profile in profiles {
            println!(
                "{0: <60} | {1: <35} | {2: <35}",
                profile.expect_remote_actor_id(),
                profile.unreachable_since
                    .expect("unreachable flag should be present")
                    .to_string(),
                profile.updated_at.to_string(),
            );
        };
        Ok(())
    }
}

/// Delete user
#[derive(Parser)]
pub struct DeleteUser {
    id_or_name: String,
}

impl DeleteUser {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let profile = get_profile_by_id_or_acct(
            db_client,
            &self.id_or_name,
        ).await?;
        if profile.is_local() {
            let user = get_user_by_id(db_client, profile.id).await?;
            delete_user(config, db_client, &user).await?;
        } else {
            let deletion_queue = delete_profile(db_client, profile.id).await?;
            deletion_queue.into_job(db_client).await?;
        };
        println!("user deleted");
        Ok(())
    }
}
