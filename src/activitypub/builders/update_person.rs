use serde::Serialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_config::Instance;
use mitra_federation::constants::AP_PUBLIC;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActor,
    relationships::queries::get_followers,
    users::queries::get_user_by_name,
    users::types::User,
};
use mitra_utils::id::generate_ulid;
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    actors::types::{build_local_actor, Actor},
    contexts::{build_default_context, Context},
    identifiers::{
        local_actor_followers,
        local_object_id,
        parse_local_actor_id,
        parse_local_object_id,
    },
    queues::OutgoingActivityJobData,
    receiver::HandlerError,
    vocabulary::{PERSON, UPDATE},
};

#[derive(Serialize)]
pub struct UpdatePerson {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Actor,

    to: Vec<String>,
    cc: Vec<String>,
}

pub fn build_update_person(
    instance_url: &str,
    user: &User,
) -> Result<UpdatePerson, DatabaseError> {
    let actor = build_local_actor(user, instance_url)?;
    // Update(Person) is idempotent so its ID can be random
    let internal_activity_id = generate_ulid();
    let activity_id = local_object_id(instance_url, &internal_activity_id);
    let activity = UpdatePerson {
        context: build_default_context(),
        activity_type: UPDATE.to_string(),
        id: activity_id,
        actor: actor.id.clone(),
        object: actor,
        to: vec![AP_PUBLIC.to_string()],
        cc: vec![local_actor_followers(instance_url, &user.profile.username)],
    };
    Ok(activity)
}

pub(super) async fn get_update_person_recipients(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
) -> Result<Vec<DbActor>, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let mut recipients = vec![];
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub async fn prepare_update_person(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    user: &User,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_update_person(
        &instance.url(),
        user,
    )?;
    let recipients = get_update_person_recipients(db_client, &user.id).await?;
    Ok(OutgoingActivityJobData::new(
        user,
        activity,
        recipients,
    ))
}

pub fn is_update_person_activity(activity: &JsonValue) -> bool {
    let maybe_activity_type = activity["type"].as_str();
    if maybe_activity_type != Some(UPDATE) {
        return false;
    };
    let maybe_object_type = activity["object"]["type"].as_str();
    if maybe_object_type != Some(PERSON) {
        return false;
    };
    true
}

pub async fn validate_update_person_c2s(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    activity: &JsonValue,
) -> Result<User, HandlerError> {
    if !is_update_person_activity(activity) {
        return Err(ValidationError("invalid activity").into());
    };
    let activity_id = activity["id"].as_str()
        .ok_or(ValidationError("invalid activity"))?;
    // TODO: verify activity ID has not been used before
    let _internal_activity_id = parse_local_object_id(
        &instance.url(),
        activity_id,
    ).map_err(|_| ValidationError("invalid activity"))?;
    let actor_id = activity["actor"].as_str()
        .ok_or(ValidationError("invalid activity"))?;
    let username = parse_local_actor_id(
        &instance.url(),
        actor_id,
    ).map_err(|_| ValidationError("invalid activity"))?;
    let user = get_user_by_name(db_client, &username).await?;
    Ok(user)
}

// TODO: remove
pub use crate::activitypub::authentication::verify_signed_c2s_activity;

pub async fn forward_update_person(
    db_client: &impl DatabaseClient,
    user: &User,
    activity: &JsonValue,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    // TODO: parse to and cc fields
    let recipients = get_update_person_recipients(db_client, &user.id).await?;
    Ok(OutgoingActivityJobData::new(
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
    fn test_build_update_person() {
        let user = User {
            profile: DbActorProfile {
                username: "testuser".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let activity = build_update_person(
            INSTANCE_URL,
            &user,
        ).unwrap();
        assert_eq!(
            activity.object.id,
            format!("{}/users/testuser", INSTANCE_URL),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC.to_string()]);
        assert_eq!(
            activity.cc,
            vec![format!("{}/users/testuser/followers", INSTANCE_URL)],
        );
    }
}
