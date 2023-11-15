use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::{
        create_post,
        delete_post,
        get_post_by_remote_object_id,
        get_repost_by_author,
    },
    posts::types::PostCreateData,
    profiles::queries::get_profile_by_remote_actor_id,
};
use mitra_services::media::MediaStorage;
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    deserialization::{deserialize_into_object_id, find_object_id},
    fetcher::helpers::{get_or_import_profile_by_actor_id, import_post},
    identifiers::parse_local_object_id,
    vocabulary::*,
};

use super::HandlerResult;

const FEP_1B12_ACTIVITIES: [&str; 10] = [
    ADD,
    BLOCK,
    CREATE,
    DELETE,
    DISLIKE,
    LIKE,
    LOCK,
    REMOVE,
    UNDO,
    UPDATE,
];

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
    match activity["object"]["type"].as_str() {
        Some(object_type) if FEP_1B12_ACTIVITIES.contains(&object_type) => {
            return handle_fep_1b12_announce(db_client, activity).await;
        },
        _ => (),
    };
    let activity: Announce = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let repost_object_id = activity.id;
    match get_post_by_remote_object_id(
        db_client,
        &repost_object_id,
    ).await {
        Ok(_) => return Ok(None), // Ignore if repost already exists
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let instance = config.instance();
    let storage = MediaStorage::from(config);
    let author = get_or_import_profile_by_actor_id(
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
            let post = import_post(
                db_client,
                &instance,
                &storage,
                activity.object,
                None,
            ).await?;
            post.id
        },
    };
    let repost_data = PostCreateData::repost(
        post_id,
        Some(repost_object_id.clone()),
    );
    match create_post(db_client, &author.id, repost_data).await {
        Ok(_) => Ok(Some(NOTE)),
        Err(DatabaseError::AlreadyExists("post")) => {
            // Ignore activity if repost already exists (with a different
            // object ID, or due to race condition in a handler).
            log::warn!("repost already exists: {}", repost_object_id);
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
    actor: String,
    object: JsonValue,
}

async fn handle_fep_1b12_announce(
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let GroupAnnounce { actor: group_id, object: activity } =
        serde_json::from_value(activity)
            .map_err(|_| ValidationError("unexpected activity structure"))?;
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("unexpected activity structure"))?;
    if activity_type == DELETE {
        let group = get_profile_by_remote_actor_id(
            db_client,
            &group_id,
        ).await?;
        let object_id = find_object_id(&activity["object"])?;
        let post_id = match get_post_by_remote_object_id(
            db_client,
            &object_id,
        ).await {
            Ok(post) => post.id,
            // Ignore Announce(Delete) if post is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        match get_repost_by_author(db_client, &post_id, &group.id).await {
            Ok(repost_id) => {
                delete_post(db_client, &repost_id).await?;
            },
            // Ignore Announce(Delete) if repost is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        Ok(Some(DELETE))
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
