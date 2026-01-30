// https://codeberg.org/fediverse/fep/src/branch/main/fep/171b/fep-171b.md
use serde::Serialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    conversations::types::Conversation,
    database::{DatabaseClient, DatabaseError},
    posts::{
        queries::get_post_by_id,
        types::{PostDetailed, Visibility},
    },
    users::{
        queries::get_user_by_id,
        types::User,
    },
};
use mitra_utils::id::generate_ulid;

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{
        local_activity_id,
        local_actor_id,
        local_conversation_history_collection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::{ADD, DELETE, ORDERED_COLLECTION},
};

use super::note::get_note_recipients;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    #[serde(rename = "type")]
    collection_type: String,
    id: String,
    attributed_to: String,
}

#[derive(Serialize)]
struct AddContextActivity {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    actor: String,
    id: String,
    object: JsonValue,
    target: Target,

    to: Vec<String>,
}

fn build_add_context_activity(
    instance_uri: &str,
    sender_username: &str,
    conversation_id: Uuid,
    conversation_audience: &str,
    activity: JsonValue,
) -> AddContextActivity {
    let actor_id = local_actor_id(instance_uri, sender_username);
    let activity_id = local_activity_id(instance_uri, ADD, generate_ulid());
    let target_id = local_conversation_history_collection(
        instance_uri,
        conversation_id,
    );
    AddContextActivity {
        context: build_default_context(),
        activity_type: ADD.to_string(),
        actor: actor_id.clone(),
        id: activity_id,
        object: activity,
        target: Target {
            id: target_id,
            collection_type: ORDERED_COLLECTION.to_string(),
            attributed_to: actor_id,
        },
        to: vec![conversation_audience.to_string()],
    }
}

async fn prepare_add_context_activity(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    conversation_owner: &User,
    conversation_id: Uuid,
    conversation_root: &PostDetailed,
    conversation_audience: &str,
    conversation_activity: JsonValue,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(conversation_owner.id, conversation_root.author.id);
    let conversation = conversation_root.expect_conversation();
    assert_eq!(conversation_id, conversation.id);
    let activity = build_add_context_activity(
        instance.uri_str(),
        &conversation_owner.profile.username,
        conversation_id,
        conversation_audience,
        conversation_activity,
    );
    let recipients = get_note_recipients(db_client, conversation_root).await?;
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        conversation_owner,
        activity,
        recipients,
    ))
}

/// Distributes activity to conversation participants if the owner is local
pub async fn sync_conversation(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    conversation: &Conversation,
    activity: JsonValue,
    activity_visibility: Visibility,
) -> Result<(), DatabaseError> {
    let root = match get_post_by_id(db_client, conversation.root_id).await {
        Ok(root) => root,
        Err(DatabaseError::NotFound(_))
            if activity["type"].as_str() == Some(DELETE) =>
        {
            // Root has been deleted; nothing to do
            return Ok(());
        },
        Err(other_error) => return Err(other_error),
    };
    if !root.is_local() {
        // Conversation owner is remote
        return Ok(());
    };
    if activity_visibility == Visibility::Conversation {
        // Conversation activities are synced.
    } else if conversation.is_public()
        && instance.federation.fep_171b_public_enabled
    {
        if activity_visibility == Visibility::Public {
            // Public activities are synced if public sync is enabled.
        } else {
            log::info!("not syncing {activity_visibility:?} activity");
            return Ok(());
        }
    } else {
        // Replies that don't conform to FEP-171b are not synced.
        // DMs are not synced.
        return Ok(());
    };
    if let Some(ref conversation_audience) = conversation.audience {
        let conversation_owner = get_user_by_id(db_client, root.author.id).await?;
        prepare_add_context_activity(
            db_client,
            instance,
            &conversation_owner,
            conversation.id,
            &root,
            conversation_audience,
            activity,
        ).await?.save_and_enqueue(db_client).await?;
    } else {
        log::warn!("conversation audience is not known");
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_add_context_activity() {
        let owner_username = "test";
        let conversation_id = generate_ulid();
        let conversation_audience = "https://social.example/users/test/followers";
        let conversation_activity = serde_json::json!({
            "type": "Create",
        });
        let activity = build_add_context_activity(
            INSTANCE_URI,
            owner_username,
            conversation_id,
            conversation_audience,
            conversation_activity.clone(),
        );
        assert_eq!(activity.activity_type, "Add");
        assert_eq!(activity.actor, "https://social.example/users/test");
        assert_eq!(activity.object, conversation_activity);
        assert_eq!(
            activity.target.id,
            format!("https://social.example/collections/conversations/{conversation_id}/history"),
        );
        assert_eq!(activity.target.attributed_to, activity.actor);
        assert_eq!(activity.to, vec![conversation_audience.to_string()]);
    }
}
