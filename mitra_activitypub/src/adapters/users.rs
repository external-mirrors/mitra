use mitra_config::Config;
use mitra_models::{
    accounts::types::User,
    activitypub::queries::save_actor,
    database::{DatabaseClient, DatabaseError},
    profiles::queries::delete_profile,
};
use mitra_services::media::MediaServer;

use crate::{
    actors::builders::build_local_actor,
    authority::Authority,
    builders::delete_person::prepare_delete_person,
};

// NOTE: not called when emojis are updated
pub async fn create_or_update_local_actor(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    account: &User,
) -> Result<(), DatabaseError> {
    let authority = Authority::from(&config.instance());
    let media_server = MediaServer::new(config);
    let actor = build_local_actor(&authority, &media_server, account)?;
    let actor_json = serde_json::to_value(&actor)
        .expect("actor should be serializable");
    save_actor(db_client, &actor.id, &actor_json, account.id).await?;
    Ok(())
}

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
