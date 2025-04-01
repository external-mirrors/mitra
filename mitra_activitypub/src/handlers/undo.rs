use serde::Deserialize;
use serde_json::Value;

use apx_sdk::{
    deserialization::{deserialize_into_object_id, object_to_id},
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::{
        delete_repost,
        get_remote_repost_by_activity_id,
    },
    profiles::queries::{
        get_remote_profile_by_actor_id,
    },
    reactions::queries::{
        delete_reaction,
        get_remote_reaction_by_activity_id,
    },
    relationships::queries::{
        get_follow_request_by_activity_id,
        unfollow,
    },
    users::queries::get_user_by_name,
};
use mitra_validators::errors::ValidationError;

use crate::{
    identifiers::{canonicalize_id, parse_local_actor_id},
    importers::{ActorIdResolver, ApClient},
    vocabulary::{ANNOUNCE, FOLLOW, LIKE},
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct UndoFollow {
    actor: String,
    object: Value,
}

/// Special handler for Undo with embedded Follow
async fn handle_undo_follow(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: UndoFollow = serde_json::from_value(activity)?;
    let canonical_actor_id = canonicalize_id(&activity.actor)?;
    let source_profile = get_remote_profile_by_actor_id(
        db_client,
        &canonical_actor_id.to_string(),
    ).await?;
    // Use object because activity ID might not be present
    let target_actor_id = object_to_id(&activity.object["object"])
        .map_err(|_| ValidationError("invalid follow activity object"))?;
    let target_username = parse_local_actor_id(
        &config.instance_url(),
        &target_actor_id,
    )?;
    let target_user = get_user_by_name(db_client, &target_username).await?;
    match unfollow(db_client, source_profile.id, target_user.id).await {
        Ok(_) => (),
        // Ignore Undo if relationship doesn't exist
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(Descriptor::object(FOLLOW)))
}

#[derive(Deserialize)]
struct Undo {
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_undo(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    if let Some(FOLLOW) = activity["object"]["type"].as_str() {
        // Undo() with nested follow activity
        return handle_undo_follow(config, db_client, activity).await;
    };

    let activity: Undo = serde_json::from_value(activity)?;
    let ap_client = ApClient::new(config, db_client).await?;
    let actor_profile = ActorIdResolver::default().only_remote().resolve(
        &ap_client,
        db_client,
        &activity.actor,
    ).await?;
    let canonical_object_id = canonicalize_id(&activity.object)?;

    match get_follow_request_by_activity_id(
        db_client,
        &canonical_object_id.to_string(),
    ).await {
        Ok(follow_request) => {
            // Undo(Follow)
            if follow_request.source_id != actor_profile.id {
                return Err(ValidationError("actor is not a follower").into());
            };
            unfollow(
                db_client,
                follow_request.source_id,
                follow_request.target_id,
            ).await?;
            return Ok(Some(Descriptor::object(FOLLOW)));
        },
        Err(DatabaseError::NotFound(_)) => (), // try other object types
        Err(other_error) => return Err(other_error.into()),
    };

    match get_remote_reaction_by_activity_id(
        db_client,
        &canonical_object_id.to_string(),
    ).await {
        Ok(reaction) => {
            // Undo(Like), Undo(EmojiReact), Undo(Dislike)
            if reaction.author_id != actor_profile.id {
                return Err(ValidationError("actor is not an author").into());
            };
            delete_reaction(
                db_client,
                reaction.author_id,
                reaction.post_id,
                reaction.content.as_deref(),
            ).await?;
            Ok(Some(Descriptor::object(LIKE)))
        },
        Err(DatabaseError::NotFound(_)) => {
            // Undo(Announce)
            let repost = match get_remote_repost_by_activity_id(
                db_client,
                &canonical_object_id.to_string(),
            ).await {
                Ok(repost) => repost,
                // Ignore undo if neither reaction nor repost is found
                Err(DatabaseError::NotFound(_)) => return Ok(None),
                Err(other_error) => return Err(other_error.into()),
            };
            if repost.author.id != actor_profile.id {
                return Err(ValidationError("actor is not an author").into());
            };
            delete_repost(db_client, repost.id).await?;
            Ok(Some(Descriptor::object(ANNOUNCE)))
        },
        Err(other_error) => Err(other_error.into()),
    }
}
