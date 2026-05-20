use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::DatabaseError,
    profiles::types::{DbActor, DbActorProfile},
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        compatible_id,
        local_activity_id,
        local_actor_id,
    },
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
    instance_uri: &str,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: Uuid,
    follow_request_has_deprecated_ap_id: bool,
) -> UndoFollow {
    let object = build_follow(
        instance_uri,
        actor_profile,
        target_actor_id,
        follow_request_id,
        follow_request_has_deprecated_ap_id,
        false, // no context
    );
    let activity_id = local_activity_id(
        instance_uri,
        UNDO,
        follow_request_id,
    );
    let actor_id = local_actor_id(instance_uri, &actor_profile.username);
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
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let target_actor_id = compatible_id(target_actor, &target_actor.id)?;
    let activity = build_undo_follow(
        instance.uri_str(),
        &sender.profile,
        &target_actor_id,
        follow_request_id,
        follow_request_has_deprecated_ap_id,
    );
    let recipients = Recipient::for_inbox(target_actor);
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, to_value};
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_undo_follow() {
        let source = DbActorProfile::local_for_test("user");
        let target_actor_id = "https://test.remote/users/123";
        let follow_request_id = generate_ulid();
        let activity = build_undo_follow(
            INSTANCE_URI,
            &source,
            target_actor_id,
            follow_request_id,
            false, // no legacy activity ID
        );
        let value = to_value(&activity).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v2",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji"
                },
            ],
            "id": format!("{}/activities/undo/{}", INSTANCE_URI, follow_request_id),
            "type": "Undo",
            "actor": activity.actor,
            "object": {
                "id": format!("{}/activities/follow/{}", INSTANCE_URI, follow_request_id),
                "type": "Follow",
                "actor": activity.actor,
                "object": target_actor_id,
                "to": [target_actor_id],
            },
            "to": [target_actor_id],
        });
        assert_eq!(value, expected_value);
    }

    #[test]
    fn test_build_undo_follow_legacy_follow_request() {
        let source = DbActorProfile::local_for_test("user");
        let target_actor_id = "https://test.remote/users/123";
        let follow_request_id = generate_ulid();
        let activity = build_undo_follow(
            INSTANCE_URI,
            &source,
            target_actor_id,
            follow_request_id,
            true, // legacy activity ID
        );
        let value = to_value(activity).unwrap();
        assert_eq!(
            value["object"]["id"].as_str().unwrap(),
            format!("{}/objects/{}", INSTANCE_URI, follow_request_id),
        );
    }
}
