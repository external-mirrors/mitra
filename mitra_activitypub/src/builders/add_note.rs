use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_federation::constants::AP_PUBLIC;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActor,
    relationships::queries::get_followers,
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
    vocabulary::ADD,
};

#[derive(Serialize)]
struct AddNote {
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

fn build_add_note(
    instance_url: &str,
    sender_username: &str,
    post_id: Uuid,
) -> AddNote {
    let actor_id = local_actor_id(instance_url, sender_username);
    let activity_id = local_activity_id(instance_url, ADD, generate_ulid());
    let object_id = local_object_id(instance_url, post_id);
    let target_id = LocalActorCollection::Featured.of(&actor_id);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    AddNote {
        context: build_default_context(),
        activity_type: ADD.to_string(),
        actor: actor_id,
        id: activity_id,
        object: object_id,
        target: target_id,
        to: vec![AP_PUBLIC.to_string()],
        cc: vec![followers],
    }
}

pub(super) async fn get_add_note_recipients(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
) -> Result<Vec<DbActor>, DatabaseError> {
    let followers = get_followers(db_client, &user_id).await?;
    let mut recipients = vec![];
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub async fn prepare_add_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    post_id: Uuid,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_add_note(
        &instance.url(),
        &sender.profile.username,
        post_id,
    );
    let recipients = get_add_note_recipients(db_client, sender.id).await?;
    Ok(OutgoingActivityJobData::new(
        &instance.url(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://social.example";

    #[test]
    fn test_build_add_note() {
        let sender_username = "local";
        let post_id = generate_ulid();
        let activity = build_add_note(
            INSTANCE_URL,
            sender_username,
            post_id,
        );
        assert_eq!(activity.activity_type, "Add");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, sender_username),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URL, post_id),
        );
        assert_eq!(
            activity.target,
            format!("{INSTANCE_URL}/users/{sender_username}/collections/featured"),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(
            activity.cc[0],
            format!("{INSTANCE_URL}/users/{sender_username}/followers"),
        );
    }
}
