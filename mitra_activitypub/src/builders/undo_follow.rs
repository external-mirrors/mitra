use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    profiles::types::{DbActor, DbActorProfile},
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{local_activity_id, local_actor_id},
    queues::OutgoingActivityJobData,
    vocabulary::UNDO,
};

use super::follow::{build_follow, Follow};

#[derive(Serialize)]
struct UndoFollow {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Follow,

    to: Vec<String>,
}

fn build_undo_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: Uuid,
    follow_request_has_deprecated_ap_id: bool,
) -> UndoFollow {
    let object = build_follow(
        instance_url,
        actor_profile,
        target_actor_id,
        follow_request_id,
        follow_request_has_deprecated_ap_id,
        false, // no context
    );
    let activity_id = local_activity_id(
        instance_url,
        UNDO,
        follow_request_id,
    );
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    UndoFollow {
        _context: build_default_context(),
        activity_type: UNDO.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object,
        to: vec![target_actor_id.to_string()],
    }
}

pub fn prepare_undo_follow(
    instance: &Instance,
    sender: &User,
    target_actor: &DbActor,
    follow_request_id: Uuid,
    follow_request_has_deprecated_ap_id: bool,
) -> OutgoingActivityJobData {
    let activity = build_undo_follow(
        &instance.url(),
        &sender.profile,
        &target_actor.id,
        follow_request_id,
        follow_request_has_deprecated_ap_id,
    );
    let recipients = vec![target_actor.clone()];
    OutgoingActivityJobData::new(
        &instance.url(),
        sender,
        activity,
        recipients,
    )
}

#[cfg(test)]
mod tests {
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_undo_follow() {
        let source = DbActorProfile::local_for_test("user");
        let target_actor_id = "https://test.remote/users/123";
        let follow_request_id = generate_ulid();
        let activity = build_undo_follow(
            INSTANCE_URL,
            &source,
            target_actor_id,
            follow_request_id,
            true, // legacy activity ID
        );

        assert_eq!(
            activity.id,
            format!("{}/activities/undo/{}", INSTANCE_URL, follow_request_id),
        );
        assert_eq!(activity.activity_type, "Undo");
        assert_eq!(
            activity.actor,
            format!("{}/users/user", INSTANCE_URL),
        );
        assert_eq!(activity.object._context, None);
        assert_eq!(
            activity.object.id,
            format!("{}/objects/{}", INSTANCE_URL, follow_request_id),
        );
        assert_eq!(activity.object.actor, activity.actor);
        assert_eq!(activity.object.object, target_actor_id);
        assert_eq!(activity.to, vec![target_actor_id]);

        let activity = build_undo_follow(
            INSTANCE_URL,
            &source,
            target_actor_id,
            follow_request_id,
            false, // no legacy activity ID
        );
        assert_eq!(
            activity.object.id,
            format!("{}/activities/follow/{}", INSTANCE_URL, follow_request_id),
        );
    }
}
