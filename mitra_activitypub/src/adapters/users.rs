use mitra_config::Config;
use mitra_models::{
    accounts::types::ManagedAccount,
    activitypub::queries::save_actor,
    database::{DatabaseClient, DatabaseError},
    profiles::{
        queries::delete_profile,
        types::{DbActor, DbActorProfile},
    },
};
use mitra_services::media::MediaServer;

use crate::{
    actors::builders::{
        build_local_actor,
        local_actor_data,
    },
    authority::{Authority, AuthorityRoot},
    builders::delete_person::prepare_delete_person,
    identifiers::local_actor_id_canonical,
};

pub fn get_actor_data(
    authority_root: &AuthorityRoot,
    profile: &DbActorProfile,
) -> DbActor {
    if let Some(ref actor_data) = profile.actor_json {
        actor_data.clone()
    } else {
        local_actor_data(authority_root, profile)
    }
}

// NOTE: not called when emojis are updated
pub async fn create_or_update_local_actor(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    account: &impl ManagedAccount,
) -> Result<(), DatabaseError> {
    let authority = Authority::from(&config.instance());
    let media_server = MediaServer::new(config);
    let actor_id = local_actor_id_canonical(
        authority.root(),
        account.id(),
        &account.profile().username,
    );
    let actor = build_local_actor(&authority, &media_server, account)
        .map_err(|_| DatabaseError::type_error())?;
    let actor_json = serde_json::to_value(&actor)
        .expect("actor should be serializable");
    save_actor(db_client, &actor_id, &actor_json, account.id()).await?;
    Ok(())
}

pub async fn delete_account(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    account: &impl ManagedAccount,
) -> Result<(), DatabaseError> {
    let activity = prepare_delete_person(
        db_client,
        &config.instance(),
        account,
    ).await?;
    let deletion_queue = delete_profile(db_client, account.id()).await?;
    deletion_queue.into_job(db_client).await?;
    activity.save_and_enqueue(db_client).await?;
    Ok(())
}
