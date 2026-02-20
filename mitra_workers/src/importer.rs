use apx_sdk::addresses::WebfingerAddress;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_activitypub::{
    builders::{
        follow::follow_or_create_request,
        move_person::prepare_move_person,
        undo_follow::prepare_undo_follow,
    },
    errors::HandlerError,
    importers::{
        is_actor_importer_error,
        get_or_import_profile_by_webfinger_address,
        ApClient,
    },
};
use mitra_config::Config;
use mitra_models::{
    background_jobs::{
        queries::enqueue_job,
        types::JobType,
    },
    database::{
        db_client_await,
        get_database_client,
        DatabaseClient,
        DatabaseConnectionPool,
        DatabaseError,
    },
    notifications::helpers::create_move_notification,
    profiles::{
        queries::get_remote_profile_by_actor_id,
    },
    relationships::queries::{follow, unfollow},
    users::{
        queries::get_user_by_id,
    },
};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImporterJobData {
    Follows {
        user_id: Uuid,
        address_list: Vec<String>,
    },
    Followers {
        user_id: Uuid,
        from_actor_id: String,
        address_list: Vec<String>,
    },
}

impl ImporterJobData {
    pub async fn into_job(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        let job_data = serde_json::to_value(self)
            .expect("job data should be serializable");
        let scheduled_for = Utc::now(); // run immediately
        enqueue_job(
            db_client,
            JobType::DataImport,
            &job_data,
            scheduled_for,
        ).await?;
        Ok(())
    }
}

pub async fn import_follows_task(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    user_id: Uuid,
    address_list: Vec<String>,
) -> Result<(), anyhow::Error> {
    let user = get_user_by_id(
        db_client_await!(db_pool),
        user_id,
    ).await?;
    let ap_client = ApClient::new_with_pool(config, db_pool).await?;
    for webfinger_address in address_list {
        let webfinger_address: WebfingerAddress = webfinger_address.parse()?;
        let profile = match get_or_import_profile_by_webfinger_address(
            &ap_client,
            db_pool,
            &webfinger_address,
        ).await {
            Ok(profile) => profile,
            Err(error) if is_actor_importer_error(&error) => {
                log::warn!(
                    "failed to import profile {}: {}",
                    webfinger_address,
                    error,
                );
                continue;
            },
            Err(other_error) => return Err(other_error.into()),
        };
        if profile.id == user.id {
            continue;
        };
        let db_client = &mut **get_database_client(db_pool).await?;
        follow_or_create_request(
            db_client,
            &config.instance(),
            &user,
            &profile,
        ).await?;
    };
    Ok(())
}

pub async fn import_followers_task(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    user_id: Uuid,
    from_actor_id: String,
    address_list: Vec<String>,
) -> Result<(), anyhow::Error> {
    let user = get_user_by_id(
        db_client_await!(db_pool),
        user_id,
    ).await?;
    let maybe_from_profile = match get_remote_profile_by_actor_id(
        db_client_await!(db_pool),
        &from_actor_id,
    ).await {
        Ok(profile) => Some(profile),
        Err(DatabaseError::NotFound(_)) => None,
        Err(other_error) => return Err(other_error.into()),
    };
    let instance = config.instance();
    let ap_client = ApClient::new_with_pool(config, db_pool).await?;
    let mut remote_followers = vec![];
    for follower_address in address_list {
        let follower_address: WebfingerAddress = follower_address.parse()?;
        let follower = match get_or_import_profile_by_webfinger_address(
            &ap_client,
            db_pool,
            &follower_address,
        ).await {
            Ok(profile) => profile,
            Err(error @ (
                HandlerError::FetchError(_) |
                HandlerError::DatabaseError(DatabaseError::NotFound(_))
            )) => {
                log::warn!(
                    "failed to import profile {}: {}",
                    follower_address,
                    error,
                );
                continue;
            },
            Err(other_error) => return Err(other_error.into()),
        };
        if let Some(remote_actor) = follower.actor_json {
            // Add remote actor to activity recipients list
            remote_followers.push(remote_actor);
        } else {
            // Immediately move local followers (only if alias can be verified)
            if let Some(ref from_profile) = maybe_from_profile {
                let db_client = &mut **get_database_client(db_pool).await?;
                match unfollow(db_client, follower.id, from_profile.id).await {
                    Ok(maybe_follow_request_deleted) => {
                        // Send Undo(Follow) to a remote actor
                        let remote_actor = from_profile.actor_json.as_ref()
                            .expect("actor data must be present");
                        let (
                            follow_request_id,
                            follow_request_has_deprecated_ap_id,
                        ) = maybe_follow_request_deleted
                            .expect("follow request must exist");
                        let follower =
                            get_user_by_id(db_client, follower.id).await?;
                        prepare_undo_follow(
                            &instance,
                            &follower,
                            remote_actor,
                            follow_request_id,
                            follow_request_has_deprecated_ap_id,
                        )?.save_and_enqueue(db_client).await?;
                    },
                    // Not a follower, ignore
                    Err(DatabaseError::NotFound(_)) => continue,
                    Err(other_error) => return Err(other_error.into()),
                };
                match follow(db_client, follower.id, user.id).await {
                    Ok(_) => (),
                    // Ignore if already following
                    Err(DatabaseError::AlreadyExists(_)) => (),
                    Err(other_error) => return Err(other_error.into()),
                };
                create_move_notification(
                    db_client,
                    user.id,
                    follower.id,
                ).await?;
            };
        };
    };
    let db_client = &**get_database_client(db_pool).await?;
    prepare_move_person(
        &instance,
        &user,
        &from_actor_id,
        true, // pull mode
        remote_followers,
    ).save_and_enqueue(db_client).await?;
    Ok(())
}
