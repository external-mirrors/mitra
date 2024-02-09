use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::activitypub::{
    constants::AP_PUBLIC,
    contexts::{build_default_context, Context},
    identifiers::{
        local_actor_id,
        local_actor_featured,
        local_actor_followers,
        local_object_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::ADD,
};
use super::update_person::get_update_person_recipients;

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
    post_id: &Uuid,
) -> AddNote {
    let actor_id = local_actor_id(instance_url, sender_username);
    let activity_id = local_object_id(instance_url, &generate_ulid());
    let object_id = local_object_id(instance_url, post_id);
    let target_id = local_actor_featured(instance_url, sender_username);
    let followers = local_actor_followers(instance_url, sender_username);
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

pub async fn prepare_add_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    post_id: &Uuid,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_add_note(
        &instance.url(),
        &sender.profile.username,
        post_id,
    );
    let recipients = get_update_person_recipients(db_client, &sender.id).await?;
    Ok(OutgoingActivityJobData::new(
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
            &post_id,
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
            format!("{}/users/{}/collections/featured", INSTANCE_URL, sender_username),
        );
    }
}
