use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_federation::constants::AP_PUBLIC;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActor,
    relationships::queries::{get_followers, get_following},
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{local_activity_id, local_actor_id},
    queues::OutgoingActivityJobData,
    vocabulary::DELETE,
};

#[derive(Serialize)]
struct DeletePerson {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
}

fn build_delete_person(
    instance_url: &str,
    user: &User,
) -> DeletePerson {
    let actor_id = local_actor_id(instance_url, &user.profile.username);
    let activity_id = local_activity_id(instance_url, DELETE, user.id);
    DeletePerson {
        context: build_default_context(),
        activity_type: DELETE.to_string(),
        id: activity_id,
        actor: actor_id.clone(),
        object: actor_id,
        to: vec![AP_PUBLIC.to_string()],
    }
}

async fn get_delete_person_recipients(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
) -> Result<Vec<DbActor>, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let following = get_following(db_client, user_id).await?;
    let mut recipients = vec![];
    for profile in followers.into_iter().chain(following.into_iter()) {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub async fn prepare_delete_person(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    user: &User,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_delete_person(&instance.url(), user);
    let recipients = get_delete_person_recipients(db_client, &user.id).await?;
    Ok(OutgoingActivityJobData::new(
        &instance.url(),
        user,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_delete_person() {
        let user = User {
            profile: DbActorProfile::local_for_test("testuser"),
            ..Default::default()
        };
        let activity = build_delete_person(INSTANCE_URL, &user);
        assert_eq!(
            activity.id,
            format!("{}/activities/delete/{}", INSTANCE_URL, user.id),
        );
        assert_eq!(activity.actor, activity.object);
        assert_eq!(
            activity.object,
            format!("{}/users/testuser", INSTANCE_URL),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
    }
}
