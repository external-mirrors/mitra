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
        get_or_import_profile_by_actor_address,
    },
};
use mitra_config::Config;
use mitra_federation::addresses::ActorAddress;
use mitra_models::{
    background_jobs::{
        queries::enqueue_job,
        types::JobType,
    },
    database::{
        DatabaseClient,
        DatabaseError,
    },
    notifications::helpers::create_move_notification,
    profiles::{
        queries::get_profile_by_remote_actor_id,
        types::DbActorProfile,
    },
    relationships::queries::{
        follow,
        get_followers,
        get_following,
        unfollow,
    },
    users::{
        queries::get_user_by_id,
    },
};
use mitra_services::media::MediaStorage;
use mitra_validators::errors::ValidationError;

const IMPORTER_JOB_LIMIT: usize = 500;

fn export_profiles_to_csv(
    local_hostname: &str,
    profiles: Vec<DbActorProfile>,
) -> String {
    let mut csv = String::new();
    for profile in profiles {
        let actor_address = ActorAddress::new_unchecked(
            &profile.username,
            profile.hostname.as_deref().unwrap_or(local_hostname),
        );
        csv += &format!("{}\n", actor_address);
    };
    csv
}

pub async fn export_followers(
    db_client: &impl DatabaseClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, followers);
    Ok(csv)
}

pub async fn export_follows(
    db_client: &impl DatabaseClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let following = get_following(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, following);
    Ok(csv)
}

pub fn parse_address_list(csv: &str)
    -> Result<Vec<ActorAddress>, ValidationError>
{
    let mut addresses: Vec<_> = csv.lines()
        .filter_map(|line| line.split(',').next())
        .map(|line| line.trim().to_string())
        // Skip header and empty lines
        .filter(|line| line != "Account address" && !line.is_empty())
        .map(|line| ActorAddress::from_handle(&line))
        .collect::<Result<_, _>>()
        .map_err(|error| ValidationError(error.message()))?;
    addresses.sort();
    addresses.dedup();
    if addresses.len() > IMPORTER_JOB_LIMIT {
        return Err(ValidationError("can't process more than 500 items at once"));
    };
    Ok(addresses)
}

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
            &JobType::DataImport,
            &job_data,
            scheduled_for,
        ).await
    }
}

pub async fn import_follows_task(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    user_id: Uuid,
    address_list: Vec<String>,
) -> Result<(), anyhow::Error> {
    let user = get_user_by_id(db_client, &user_id).await?;
    let storage = MediaStorage::from(config);
    for actor_address in address_list {
        let actor_address: ActorAddress = actor_address.parse()?;
        let profile = match get_or_import_profile_by_actor_address(
            db_client,
            &config.instance(),
            &storage,
            &actor_address,
        ).await {
            Ok(profile) => profile,
            Err(error) if is_actor_importer_error(&error) => {
                log::warn!(
                    "failed to import profile {}: {}",
                    actor_address,
                    error,
                );
                continue;
            },
            Err(other_error) => return Err(other_error.into()),
        };
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
    db_client: &mut impl DatabaseClient,
    user_id: Uuid,
    from_actor_id: String,
    address_list: Vec<String>,
) -> Result<(), anyhow::Error> {
    let user = get_user_by_id(db_client, &user_id).await?;
    let maybe_from_profile = match get_profile_by_remote_actor_id(
        db_client,
        &from_actor_id,
    ).await {
        Ok(profile) => Some(profile),
        Err(DatabaseError::NotFound(_)) => None,
        Err(other_error) => return Err(other_error.into()),
    };
    let instance = config.instance();
    let storage = MediaStorage::from(config);
    let mut remote_followers = vec![];
    for follower_address in address_list {
        let follower_address: ActorAddress = follower_address.parse()?;
        let follower = match get_or_import_profile_by_actor_address(
            db_client,
            &instance,
            &storage,
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
                match unfollow(db_client, &follower.id, &from_profile.id).await {
                    Ok(maybe_follow_request_id) => {
                        // Send Undo(Follow) to a remote actor
                        let remote_actor = from_profile.actor_json.as_ref()
                            .expect("actor data must be present");
                        let follow_request_id = maybe_follow_request_id
                            .expect("follow request must exist");
                        let follower =
                            get_user_by_id(db_client, &follower.id).await?;
                        prepare_undo_follow(
                            &instance,
                            &follower,
                            remote_actor,
                            &follow_request_id,
                        ).enqueue(db_client).await?;
                    },
                    // Not a follower, ignore
                    Err(DatabaseError::NotFound(_)) => continue,
                    Err(other_error) => return Err(other_error.into()),
                };
                match follow(db_client, &follower.id, &user.id).await {
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
    prepare_move_person(
        &instance,
        &user,
        &from_actor_id,
        true, // pull mode
        remote_followers,
    ).enqueue(db_client).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActor;
    use super::*;

    #[test]
    fn test_export_profiles_to_csv() {
        let profile_1 = DbActorProfile {
            username: "user1".to_string(),
            ..Default::default()
        };
        let profile_2 = DbActorProfile {
            username: "user2".to_string(),
            hostname: Some("test.net".to_string()),
            actor_json: Some(DbActor::default()),
            ..Default::default()
        };
        let csv = export_profiles_to_csv(
            "example.org",
            vec![profile_1, profile_2],
        );
        assert_eq!(csv, "user1@example.org\nuser2@test.net\n");
    }

    #[test]
    fn test_parse_address_list() {
        let csv = concat!(
            "\nuser1@example.net\n",
            "user2@example.com  \n",
            "@user1@example.net",
        );
        let addresses = parse_address_list(csv).unwrap();
        assert_eq!(addresses.len(), 2);
        let addresses: Vec<_> = addresses.into_iter()
            .map(|address| address.to_string())
            .collect();
        assert_eq!(addresses, vec![
            "user1@example.net",
            "user2@example.com",
        ]);
    }

    #[test]
    fn test_parse_address_list_mastodon() {
        let csv = concat!(
            "Account address,Show boosts,Notify on new posts,Languages\n",
            "user1@one.test,false,false,\n",
            "user2@two.test,true,false,\n",
        );
        let addresses = parse_address_list(csv).unwrap();
        assert_eq!(addresses.len(), 2);
        let addresses: Vec<_> = addresses.into_iter()
            .map(|address| address.to_string())
            .collect();
        assert_eq!(addresses, vec![
            "user1@one.test",
            "user2@two.test",
        ]);
    }
}
