use apx_sdk::constants::AP_PUBLIC;
use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{
        local_activity_id,
        local_actor_id,
        local_object_id,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::REMOVE,
};
use super::add_note::get_add_note_recipients;

#[derive(Serialize)]
struct RemoveNote {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    actor: String,
    id: String,
    object: String,
    target: String,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_remove_note(
    instance_uri: &str,
    sender_username: &str,
    post_id: Uuid,
) -> RemoveNote {
    let actor_id = local_actor_id(instance_uri, sender_username);
    let activity_id = local_activity_id(instance_uri, REMOVE, generate_ulid());
    let object_id = local_object_id(instance_uri, post_id);
    let target_id = LocalActorCollection::Featured.of(&actor_id);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    RemoveNote {
        context: build_default_context(),
        activity_type: REMOVE.to_string(),
        actor: actor_id,
        id: activity_id,
        object: object_id,
        target: target_id,
        to: vec![AP_PUBLIC.to_string()],
        cc: vec![followers],
    }
}

pub async fn prepare_remove_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    post_id: Uuid,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_remove_note(
        instance.uri_str(),
        &sender.profile.username,
        post_id,
    );
    let recipients = get_add_note_recipients(db_client, sender.id).await?;
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_remove_note() {
        let sender_username = "local";
        let post_id = generate_ulid();
        let activity = build_remove_note(
            INSTANCE_URI,
            sender_username,
            post_id,
        );
        assert_eq!(activity.activity_type, "Remove");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URI, sender_username),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URI, post_id),
        );
        assert_eq!(
            activity.target,
            format!("{}/users/{}/collections/featured", INSTANCE_URI, sender_username),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(
            activity.cc,
            vec![format!("{INSTANCE_URI}/users/{sender_username}/followers")],
        );
    }
}
