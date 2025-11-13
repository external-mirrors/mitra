use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::delete_profile,
    users::types::User,
};

use crate::{
    builders::delete_person::prepare_delete_person,
};

pub async fn delete_user(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    user: &User,
) -> Result<(), DatabaseError> {
    let activity = prepare_delete_person(
        db_client,
        &config.instance(),
        user,
    ).await?;
    let deletion_queue = delete_profile(db_client, user.id).await?;
    deletion_queue.into_job(db_client).await?;
    activity.save_and_enqueue(db_client).await?;
    Ok(())
}
