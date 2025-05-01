use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::deserialization::deserialize_into_object_id;
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    notifications::helpers::create_follow_request_notification,
    relationships::queries::{
        create_remote_follow_request_opt,
        follow_request_accepted,
        has_relationship,
    },
    relationships::types::RelationshipType,
    users::queries::get_user_by_id,
};
use mitra_validators::{
    activitypub::validate_any_object_id,
};

use crate::{
    builders::accept_follow::prepare_accept_follow,
    identifiers::canonicalize_id,
    importers::{get_profile_by_actor_id, ActorIdResolver, ApClient},
};

use super::{Descriptor, HandlerResult};

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
    activity: JsonValue,
) -> HandlerResult {
    // Follow(Person)
    let activity: Follow = serde_json::from_value(activity)?;
    let ap_client = ApClient::new(config, db_client).await?;
    let source_profile = ActorIdResolver::default().only_remote().resolve(
        &ap_client,
        db_client,
        &activity.actor,
    ).await?;
    let source_actor = source_profile.actor_json
        .expect("actor data should be present");
    let canonical_object_id = canonicalize_id(&activity.object)?;
    let target_profile = get_profile_by_actor_id(
        db_client,
        &config.instance_url(),
        &canonical_object_id.to_string(),
    ).await?;
    // Create new follow request or update activity ID on existing one,
    // because latest activity ID might be needed to process Undo(Follow)
    let canonical_activity_id = canonicalize_id(&activity.id)?;
    validate_any_object_id(&canonical_activity_id.to_string())?;
    let follow_request = create_remote_follow_request_opt(
        db_client,
        source_profile.id,
        target_profile.id,
        &canonical_activity_id.to_string(),
    ).await?;
    let target_user = if target_profile.is_local() {
        get_user_by_id(db_client, target_profile.id).await?
    } else {
        // Activity has been performed by a portable account
        return Ok(Some(Descriptor::object("Actor")));
    };
    let is_following = has_relationship(
        db_client,
        follow_request.source_id,
        follow_request.target_id,
        RelationshipType::Follow,
    ).await?;
    if !is_following && target_user.profile.manually_approves_followers {
        create_follow_request_notification(
            db_client,
            follow_request.source_id,
            follow_request.target_id,
        ).await?;
    } else {
        match follow_request_accepted(db_client, follow_request.id).await {
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
            &canonical_activity_id.to_string(),
        )?.save_and_enqueue(db_client).await?;
    };
    Ok(Some(Descriptor::object("Actor")))
}
