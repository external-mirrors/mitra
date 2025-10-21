use apx_sdk::{
    constants::AP_PUBLIC,
    deserialization::{
        deserialize_into_id_array,
        deserialize_into_object_id,
        object_to_id,
    },
    utils::is_activity,
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    posts::queries::{
        create_post,
        delete_repost,
        get_post_by_id,
        get_remote_post_by_object_id,
        get_remote_repost_by_activity_id,
        get_repost_by_author,
    },
    posts::types::{PostCreateData, Visibility},
    profiles::types::DbActor,
};
use mitra_validators::{
    errors::ValidationError,
    posts::validate_repost_data,
};

use crate::{
    identifiers::parse_local_object_id,
    importers::{import_post, ActorIdResolver, ApClient},
    ownership::{
        get_object_id,
        is_local_origin,
        is_same_origin,
        verify_activity_owner,
    },
    vocabulary::*,
};

use super::{
    create::handle_create,
    like::handle_like,
    note::normalize_audience,
    undo::handle_undo,
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

    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    to: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    cc: Vec<String>,
}

fn get_repost_visibility(
    actor: &DbActor,
    audience: &[String],
) -> Result<Visibility, ValidationError> {
    let normalized_audience = normalize_audience(audience)?;
    let visibility = if normalized_audience.iter()
        .any(|target_id| target_id.to_string() == AP_PUBLIC)
    {
        Visibility::Public
    } else if normalized_audience.iter()
        .any(|target_id| Some(target_id.to_string()) == actor.followers)
    {
        log::warn!("repost is not public");
        Visibility::Followers
    } else {
        return Err(ValidationError("invalid repost visibility"));
    };
    Ok(visibility)
}

pub async fn handle_announce(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    if is_activity(&activity["object"]) {
        return handle_fep_1b12_announce(config, db_pool, activity).await;
    };
    let announce: Announce = serde_json::from_value(activity)?;
    match get_remote_repost_by_activity_id(
        db_client_await!(db_pool),
        &announce.id,
    ).await {
        Ok(_) => return Ok(None), // Ignore if repost already exists
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let ap_client = ApClient::new_with_pool(config, db_pool).await?;
    let author = ActorIdResolver::default().only_remote().resolve_with_pool(
        &ap_client,
        db_pool,
        &announce.actor,
    ).await?;
    let post = match parse_local_object_id(
        ap_client.instance.uri_str(),
        &announce.object,
    ) {
        Ok(post_id) => {
            let db_client = &**get_database_client(db_pool).await?;
            get_post_by_id(db_client, post_id).await?
        },
        Err(_) => {
            // Try to get remote post
            import_post(
                &ap_client,
                db_pool,
                announce.object,
                None,
            ).await?
        },
    };
    if !post.is_public() {
        return Err(DatabaseError::NotFound("post").into());
    };
    let visibility = get_repost_visibility(
        author.expect_actor_data(),
        &[announce.to.clone(), announce.cc.clone()].concat(),
    )?;
    let repost_data = PostCreateData::repost(
        post.id,
        visibility,
        Some(announce.id.clone()),
    );
    validate_repost_data(&repost_data)?;
    let db_client = &mut **get_database_client(db_pool).await?;
    match create_post(db_client, author.id, repost_data).await {
        Ok(_) => Ok(Some(Descriptor::object("Object"))),
        Err(DatabaseError::AlreadyExists("post")) => {
            // Ignore activity if repost already exists (with a different
            // activity ID, or due to race condition in a handler).
            log::warn!("repost already exists: {}", announce.id);
            Ok(None)
        },
        // May return "post not found" error if post is not public
        Err(other_error) => Err(other_error.into()),
    }
}

/// Wrapped activities from Lemmy
/// https://codeberg.org/fediverse/fep/src/branch/main/fep/1b12/fep-1b12.md
#[derive(Deserialize)]
struct GroupAnnounce {
    id: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: JsonValue,
}

async fn handle_fep_1b12_announce(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    announce: JsonValue,
) -> HandlerResult {
    let GroupAnnounce { id: announce_id, actor: group_id, object: activity } =
        serde_json::from_value(announce)?;
    verify_activity_owner(&activity)?;
    let activity_id = get_object_id(&activity)?;
    if is_local_origin(&config.instance(), activity_id) {
        // Ignore local activities
        return Ok(None);
    };
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("unexpected activity structure"))?;
    if activity_type != DELETE && !config.federation.fep_1b12_full_enabled {
        return Ok(None);
    };
    match activity_type {
        CREATE | DELETE | DISLIKE | LIKE | UNDO | UPDATE => (),
        _ => {
            log::warn!("activity is not supported: Announce({activity_type})");
            return Ok(None);
        },
    };
    let ap_client = ApClient::new_with_pool(config, db_pool).await?;
    let activity = if is_same_origin(&announce_id, activity_id)? {
        // Embedded activity can be trusted; don't fetch
        activity.clone()
    } else {
        match ap_client.fetch_object(activity_id).await {
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
    let group = ActorIdResolver::default().only_remote().resolve_with_pool(
        &ap_client,
        db_pool,
        &group_id,
    ).await?;
    match activity_type {
        DELETE => {
            let db_client = &mut **get_database_client(db_pool).await?;
            let object_id = object_to_id(&activity["object"])
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
                Ok(repost) => {
                    delete_repost(db_client, repost.id).await?;
                },
                // Ignore Announce(Delete) if repost is not found
                Err(DatabaseError::NotFound(_)) => return Ok(None),
                Err(other_error) => return Err(other_error.into()),
            };
            Ok(Some(Descriptor::object(activity_type)))
        },
        CREATE => {
            let maybe_object_type = handle_create(
                config,
                db_pool,
                activity.clone(),
                None, // no sender (spam check will not be performed)
                true, // authenticated (by embedding or fetched from origin)
            )
                .await?
                .map(|desc| desc.to_string());
            if let Some(ARTICLE | NOTE | PAGE) = maybe_object_type.as_deref() {
                // Create repost
                let db_client = &mut **get_database_client(db_pool).await?;
                let object_id = object_to_id(&activity["object"])
                    .map_err(|_| ValidationError("invalid activity object"))?;
                let post = get_remote_post_by_object_id(
                    db_client,
                    &object_id,
                ).await?;
                if post.is_public() && post.in_reply_to_id.is_none() {
                    let repost_data = PostCreateData::repost(
                        post.id,
                        Visibility::Public,
                        Some(announce_id),
                    );
                    validate_repost_data(&repost_data)?;
                    match create_post(db_client, group.id, repost_data).await {
                        Ok(_) => (),
                        // Announce(Note) was sent too
                        Err(DatabaseError::AlreadyExists("post")) => (),
                        Err(other_error) => return Err(other_error.into()),
                    };
                };
            };
            Ok(Some(Descriptor::object(activity_type)))
        },
        LIKE | DISLIKE => {
            let maybe_type = handle_like(config, db_pool, activity).await?;
            Ok(maybe_type.map(|_| Descriptor::object(activity_type)))
        },
        UNDO => {
            let maybe_type = handle_undo(config, db_pool, activity).await?;
            Ok(maybe_type.map(|_| Descriptor::object(activity_type)))
        },
        UPDATE => {
            let maybe_type = handle_update(
                config,
                db_pool,
                activity,
                true, // authenticated (by embedding or fetched from origin)
            ).await?;
            Ok(maybe_type.map(|_| Descriptor::object(activity_type)))
        },
        _ => {
            // Ignore other activities
            Ok(None)
        },
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
        let announce: Announce = serde_json::from_value(activity_raw).unwrap();
        assert_eq!(announce.object, "https://test.org/objects/999");
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
        let announce: Announce = serde_json::from_value(activity_raw).unwrap();
        assert_eq!(announce.object, "https://test.org/objects/999");
    }
}
