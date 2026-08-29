use apx_sdk::deserialization::deserialize_into_object_id;
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_adapters::groups::can_delete_group_post;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    moderation_actions::helpers::on_local_post_deleted,
    posts::queries::delete_post,
    profiles::queries::{
        delete_profile,
        get_remote_profile_by_actor_id,
    },
};
use mitra_validators::{
    errors::ValidationError,
    moderation_actions::validate_action_reason,
};

use crate::{
    authority::Authority,
    builders::add_context_activity::sync_conversation,
    identifiers::canonicalize_id,
    importers::{
        get_post_by_object_id,
        ApClient,
    },
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Delete {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
    summary: Option<String>,
}

enum PermissionType {
    Owner,
    GroupModerator,
}

pub async fn handle_delete(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let delete: Delete = serde_json::from_value(activity.clone())?;
    let authority = Authority::from(&ap_client.instance);
    let db_client = &mut **get_database_client(db_pool).await?;
    let canonical_actor_id = canonicalize_id(&delete.actor)?;
    let canonical_object_id = canonicalize_id(&delete.object)?;
    if canonical_object_id == canonical_actor_id {
        // Self-delete
        let profile = match get_remote_profile_by_actor_id(
            db_client,
            &canonical_object_id.to_string(),
        ).await {
            Ok(profile) => profile,
            // Ignore Delete(Person) if profile is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        let deletion_queue = delete_profile(db_client, profile.id).await?;
        deletion_queue.into_job(db_client).await?;
        log::info!("deleted remote actor {}", delete.object);
        return Ok(Some(Descriptor::object("Actor")));
    };
    // Delete(Note)
    let post = match get_post_by_object_id(
        db_client,
        &authority,
        &canonical_object_id,
    ).await {
        Ok(post) => post,
        // Ignore Delete(Note) if post is not found
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let canonical_actor_id = canonicalize_id(&delete.actor)?;
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &canonical_actor_id.to_string(),
    ).await?;
    let maybe_permission = if actor_profile.id == post.author.id {
        Some(PermissionType::Owner)
    } else {
        if let Some(ref group) = post.group {
            if can_delete_group_post(db_client, &actor_profile, group).await? {
                Some(PermissionType::GroupModerator)
            } else {
                None
            }
        } else {
            None
        }
    };
    let permission = maybe_permission
        .ok_or(ValidationError("actor doesn't have permission to delete object"))?;
    let deletion_queue = delete_post(db_client, post.id).await?;
    deletion_queue.into_job(db_client).await?;
    if post.is_local() && matches!(permission, PermissionType::GroupModerator) {
        // Deleted by moderator
        let maybe_reason = delete.summary
            .filter(|summary| validate_action_reason(summary).is_ok());
        on_local_post_deleted(
            db_client,
            actor_profile.id,
            post.author.id,
            maybe_reason,
        ).await?;
    };
    sync_conversation(
        db_client,
        &ap_client.instance,
        post.expect_conversation(),
        activity,
        post.visibility,
    ).await?;
    Ok(Some(Descriptor::object("Object")))
}
