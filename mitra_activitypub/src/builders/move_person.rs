use serde::Serialize;

use mitra_config::Instance;
use mitra_federation::constants::AP_PUBLIC;
use mitra_models::{
    profiles::types::DbActor,
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{
        local_actor_id,
        local_object_id,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::MOVE,
};

#[derive(Serialize)]
struct MovePerson {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,
    target: String,

    to: String,
    cc: String,
}

/// https://codeberg.org/fediverse/fep/src/branch/main/fep/7628/fep-7628.md
fn build_move_person(
    instance_url: &str,
    sender: &User,
    from_actor_id: &str,
) -> MovePerson {
    // Move(Person) is idempotent so its ID can be random
    let internal_activity_id = generate_ulid();
    let activity_id = local_object_id(instance_url, &internal_activity_id);
    let actor_id = local_actor_id(instance_url, &sender.profile.username);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    // pull mode: actor == target
    let object_id = from_actor_id.to_string();
    let target_id = actor_id.clone();
    MovePerson {
        context: build_default_context(),
        activity_type: MOVE.to_string(),
        id: activity_id,
        actor: actor_id.clone(),
        object: object_id,
        target: target_id,
        to: AP_PUBLIC.to_string(),
        cc: followers,
    }
}

pub fn prepare_move_person(
    instance: &Instance,
    sender: &User,
    from_actor_id: &str,
    followers: Vec<DbActor>,
) -> OutgoingActivityJobData {
    let activity = build_move_person(
        &instance.url(),
        sender,
        from_actor_id,
    );
    OutgoingActivityJobData::new(
        sender,
        activity,
        followers,
    )
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_URL: &str = "https://social.example";

    #[test]
    fn test_build_move_person() {
        let sender = User {
            profile: DbActorProfile {
                username: "testuser".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let from_actor_id = "https://server0.org/users/test";
        let activity = build_move_person(
            INSTANCE_URL,
            &sender,
            from_actor_id,
        );

        assert_eq!(activity.activity_type, "Move");
        assert_eq!(
            activity.actor,
            "https://social.example/users/testuser",
        );
        assert_eq!(activity.object, from_actor_id);
        assert_eq!(activity.target, activity.actor);
        assert_eq!(
            activity.to,
            "https://www.w3.org/ns/activitystreams#Public",
        );
        assert_eq!(
            activity.cc,
            "https://social.example/users/testuser/followers",
        );
    }
}
