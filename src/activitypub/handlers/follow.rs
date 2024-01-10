use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;
use mitra_federation::deserialization::deserialize_into_object_id;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    notifications::helpers::create_follow_request_notification,
    relationships::queries::{
        create_remote_follow_request_opt,
        follow_request_accepted,
        has_relationship,
    },
    relationships::types::RelationshipType,
    users::queries::get_user_by_name,
};
use mitra_services::media::MediaStorage;
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    builders::accept_follow::prepare_accept_follow,
    identifiers::parse_local_actor_id,
    importers::get_or_import_profile_by_actor_id,
    vocabulary::PERSON,
};

use super::{HandlerError, HandlerResult};

#[derive(Deserialize)]
struct Follow {
    id: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_follow(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    // Follow(Person)
    let activity: Follow = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let source_profile = get_or_import_profile_by_actor_id(
        db_client,
        &config.instance(),
        &MediaStorage::from(config),
        &activity.actor,
    ).await?;
    let source_actor = source_profile.actor_json
        .ok_or(HandlerError::LocalObject)?;
    let target_username = parse_local_actor_id(
        &config.instance_url(),
        &activity.object,
    )?;
    let target_user = get_user_by_name(db_client, &target_username).await?;
    // Create new follow request or update activity ID on existing one,
    // because latest activity ID might be needed to process Undo(Follow)
    let follow_request = create_remote_follow_request_opt(
        db_client,
        &source_profile.id,
        &target_user.id,
        &activity.id,
    ).await?;
    let is_following = has_relationship(
        db_client,
        &follow_request.source_id,
        &follow_request.target_id,
        RelationshipType::Follow,
    ).await?;
    if !is_following && target_user.profile.manually_approves_followers {
        create_follow_request_notification(
            db_client,
            &follow_request.source_id,
            &follow_request.target_id,
        ).await?;
    } else {
        match follow_request_accepted(db_client, &follow_request.id).await {
            Ok(_) => (),
            // Proceed even if relationship already exists
            Err(DatabaseError::AlreadyExists(_)) => (),
            Err(other_error) => return Err(other_error.into()),
        };
        // Send Accept activity even if follow request has already been processed
        prepare_accept_follow(
            &config.instance(),
            &target_user,
            &source_actor,
            &activity.id,
        ).enqueue(db_client).await?;
    };
    Ok(Some(PERSON))
}
