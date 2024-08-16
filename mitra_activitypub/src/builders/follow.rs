use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    notifications::helpers::create_follow_request_notification,
    profiles::types::{DbActor, DbActorProfile},
    relationships::{
        helpers::create_follow_request,
        queries::follow,
    },
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{
        compatible_id,
        local_activity_id,
        local_actor_id,
        local_object_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::FOLLOW,
};

#[derive(Serialize)]
pub(super) struct Follow {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub _context: Option<Context>,

    #[serde(rename = "type")]
    pub activity_type: String,

    pub id: String,
    pub actor: String,
    pub object: String,

    pub to: Vec<String>,
}

pub(super) fn build_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: Uuid,
    follow_request_has_deprecated_ap_id: bool,
    with_context: bool,
) -> Follow {
    let activity_id = if follow_request_has_deprecated_ap_id {
        local_object_id(instance_url, follow_request_id)
    } else {
        local_activity_id(instance_url, FOLLOW, follow_request_id)
    };
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    Follow {
        _context: with_context.then(build_default_context),
        activity_type: FOLLOW.to_string(),
        id: activity_id,
        actor: actor_id,
        object: target_actor_id.to_string(),
        to: vec![target_actor_id.to_string()],
    }
}

fn prepare_follow(
    instance: &Instance,
    sender: &User,
    target_actor: &DbActor,
    follow_request_id: Uuid,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let target_actor_id = compatible_id(target_actor, &target_actor.id)?;
    let activity = build_follow(
        &instance.url(),
        &sender.profile,
        &target_actor_id,
        follow_request_id,
        false, // don't use legacy activity IDs
        true, // with context
    );
    let recipients = vec![target_actor.clone()];
    Ok(OutgoingActivityJobData::new(
        &instance.url(),
        sender,
        activity,
        recipients,
    ))
}

pub async fn follow_or_create_request(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    current_user: &User,
    target_profile: &DbActorProfile,
) -> Result<(), DatabaseError> {
    if target_profile.manually_approves_followers || !target_profile.is_local() {
        // Create follow request if target requires approval or it is remote
        match create_follow_request(
            db_client,
            current_user.id,
            target_profile.id,
        ).await {
            Ok(follow_request) => {
                if let Some(ref remote_actor) = target_profile.actor_json {
                    prepare_follow(
                        instance,
                        current_user,
                        remote_actor,
                        follow_request.id,
                    )?.save_and_enqueue(db_client).await?;
                } else {
                    create_follow_request_notification(
                        db_client,
                        current_user.id,
                        target_profile.id,
                    ).await?;
                };
            },
            // Do nothing if request has already been sent,
            // or if already following
            Err(DatabaseError::AlreadyExists(_)) => (),
            Err(other_error) => return Err(other_error),
        };
    } else {
        match follow(db_client, current_user.id, target_profile.id).await {
            Ok(_) => (),
            Err(DatabaseError::AlreadyExists(_)) => (), // already following
            Err(other_error) => return Err(other_error),
        };
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_follow() {
        let follower = DbActorProfile::local_for_test("follower");
        let follow_request_id = generate_ulid();
        let target_actor_id = "https://test.remote/actor/test";
        let activity = build_follow(
            INSTANCE_URL,
            &follower,
            target_actor_id,
            follow_request_id,
            false, // don't use legacy activity IDs
            true, // with context
        );

        assert_eq!(activity._context.is_some(), true);
        assert_eq!(
            activity.id,
            format!("{}/activities/follow/{}", INSTANCE_URL, follow_request_id),
        );
        assert_eq!(activity.activity_type, "Follow");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, follower.username),
        );
        assert_eq!(activity.object, target_actor_id);
        assert_eq!(activity.to, vec![target_actor_id]);
    }
}
