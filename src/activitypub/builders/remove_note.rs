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
    deliverer::OutgoingActivity,
    identifiers::{
        local_actor_id,
        local_actor_featured,
        local_actor_followers,
        local_object_id,
    },
    types::{build_default_context, Context},
    vocabulary::REMOVE,
};
use super::update_person::get_update_person_recipients;

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
    instance_url: &str,
    sender_username: &str,
    post_id: &Uuid,
) -> RemoveNote {
    let actor_id = local_actor_id(instance_url, sender_username);
    let activity_id = local_object_id(instance_url, &generate_ulid());
    let object_id = local_object_id(instance_url, post_id);
    let target_id = local_actor_featured(instance_url, sender_username);
    let followers = local_actor_followers(instance_url, sender_username);
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
    post_id: &Uuid,
) -> Result<OutgoingActivity, DatabaseError> {
    let activity = build_remove_note(
        &instance.url(),
        &sender.profile.username,
        post_id,
    );
    let recipients = get_update_person_recipients(db_client, &sender.id).await?;
    Ok(OutgoingActivity::new(
        instance,
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
    fn test_build_remove_note() {
        let sender_username = "local";
        let post_id = generate_ulid();
        let activity = build_remove_note(
            INSTANCE_URL,
            sender_username,
            &post_id,
        );
        assert_eq!(activity.activity_type, "Remove");
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
