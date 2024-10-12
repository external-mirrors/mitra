use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    deserialization::{deserialize_into_object_id, get_object_id},
    utils::is_activity,
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::{
        create_post,
        delete_repost,
        get_remote_post_by_object_id,
        get_remote_repost_by_activity_id,
        get_repost_by_author,
    },
    posts::types::PostCreateData,
    profiles::queries::get_remote_profile_by_actor_id,
};
use mitra_services::media::MediaStorage;
use mitra_validators::{
    activitypub::validate_object_id,
    errors::ValidationError,
};

use crate::{
    agent::build_federation_agent,
    filter::FederationFilter,
    identifiers::parse_local_object_id,
    importers::{fetch_any_object, import_post, ActorIdResolver},
    ownership::{is_embedded_activity_trusted, verify_activity_owner},
    vocabulary::*,
};

use super::{
    create::handle_create,
    like::handle_like,
    update::handle_update,
    Descriptor,
    HandlerResult,
};

#[derive(Deserialize)]
struct Announce {
    id: String,
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_announce(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    if is_activity(&activity["object"]) {
        return handle_fep_1b12_announce(config, db_client, activity).await;
    };
    let activity: Announce = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    match get_remote_repost_by_activity_id(
        db_client,
        &activity.id,
    ).await {
        Ok(_) => return Ok(None), // Ignore if repost already exists
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let instance = config.instance();
    let storage = MediaStorage::from(config);
    let author = ActorIdResolver::default().only_remote().resolve(
        db_client,
        &instance,
        &storage,
        &activity.actor,
    ).await?;
    let post_id = match parse_local_object_id(
        &instance.url(),
        &activity.object,
    ) {
        Ok(post_id) => post_id,
        Err(_) => {
            // Try to get remote post
            let filter = FederationFilter::init(config, db_client).await?;
            let post = import_post(
                db_client,
                &filter,
                &instance,
                &storage,
                activity.object,
                None,
            ).await?;
            post.id
        },
    };
    validate_object_id(&activity.id)?;
    let repost_data = PostCreateData::repost(
        post_id,
        Some(activity.id.clone()),
    );
    match create_post(db_client, author.id, repost_data).await {
        Ok(_) => Ok(Some(Descriptor::object("Object"))),
        Err(DatabaseError::AlreadyExists("post")) => {
            // Ignore activity if repost already exists (with a different
            // activity ID, or due to race condition in a handler).
            log::warn!("repost already exists: {}", activity.id);
            Ok(None)
        },
        // May return "post not found" error if post if not public
        Err(other_error) => Err(other_error.into()),
    }
}

/// Wrapped activities from Lemmy
/// https://codeberg.org/fediverse/fep/src/branch/main/fep/1b12/fep-1b12.md
#[derive(Deserialize)]
struct GroupAnnounce {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: JsonValue,
}

async fn handle_fep_1b12_announce(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    announce: JsonValue,
) -> HandlerResult {
    let is_embedded_trusted = is_embedded_activity_trusted(&announce)?;
    let GroupAnnounce { actor: group_id, object: activity } =
        serde_json::from_value(announce)
            .map_err(|_| ValidationError("unexpected activity structure"))?;
    verify_activity_owner(&activity)?;
    let activity_id = activity["id"].as_str()
        .ok_or(ValidationError("unexpected activity structure"))?;
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("unexpected activity structure"))?;
    if activity_type != DELETE && !config.federation.fep_1b12_full_enabled {
        return Ok(None);
    };
    match activity_type {
        CREATE | DELETE | DISLIKE | LIKE | UPDATE => (),
        _ => {
            log::warn!("activity is not supported: Announce({activity_type})");
            return Ok(None);
        },
    };
    let activity = if is_embedded_trusted {
        // Don't fetch
        activity.clone()
    } else {
        let instance = config.instance();
        let agent = build_federation_agent(&instance, None);
        match fetch_any_object(&agent, activity_id).await {
            Ok(activity) => {
                log::info!("fetched activity {}", activity_id);
                activity
            },
            Err(error) => {
                // Wrapped activities are not always available
                log::warn!("failed to fetch activity ({error}): {activity_id}");
                return Ok(None);
            },
        }
    };
    verify_activity_owner(&activity)?;
    if activity_type == DELETE {
        let group = get_remote_profile_by_actor_id(
            db_client,
            &group_id,
        ).await?;
        let object_id = get_object_id(&activity["object"])
            .map_err(|_| ValidationError("invalid activity object"))?;
        let post_id = match get_remote_post_by_object_id(
            db_client,
            &object_id,
        ).await {
            Ok(post) => post.id,
            // Ignore Announce(Delete) if post is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        // Don't delete post, only remove announcement
        // https://join-lemmy.org/docs/contributors/05-federation.html#delete-post-or-comment
        match get_repost_by_author(db_client, post_id, group.id).await {
            Ok((repost_id, _)) => {
                delete_repost(db_client, repost_id).await?;
            },
            // Ignore Announce(Delete) if repost is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        Ok(Some(Descriptor::object(activity_type)))
    } else if activity_type == CREATE {
        handle_create(
            config,
            db_client,
            activity,
            false, // not authenticated; object will be fetched
            true, // don't perform spam check
        ).await?;
        Ok(Some(Descriptor::object(activity_type)))
    } else if activity_type == LIKE || activity_type == DISLIKE {
        let maybe_type = handle_like(config, db_client, activity).await?;
        Ok(maybe_type.map(|_| Descriptor::object(activity_type)))
    } else if activity_type == UPDATE {
        let maybe_type = handle_update(
            config,
            db_client,
            activity,
            false, // not authenticated; object will be fetched
        ).await?;
        Ok(maybe_type.map(|_| Descriptor::object(activity_type)))
    } else {
        // Ignore other activities
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_deserialize_announce() {
        let activity_raw = json!({
            "type": "Announce",
            "id": "https://example.com/activities/321",
            "actor": "https://example.com/users/1",
            "object": "https://test.org/objects/999",
        });
        let activity: Announce = serde_json::from_value(activity_raw).unwrap();
        assert_eq!(activity.object, "https://test.org/objects/999");
    }

    #[test]
    fn test_deserialize_announce_nested() {
        let activity_raw = json!({
            "type": "Announce",
            "id": "https://example.com/activities/321",
            "actor": "https://example.com/users/1",
            "object": {
                "type": "Note",
                "id": "https://test.org/objects/999",
            },
        });
        let activity: Announce = serde_json::from_value(activity_raw).unwrap();
        assert_eq!(activity.object, "https://test.org/objects/999");
    }
}
