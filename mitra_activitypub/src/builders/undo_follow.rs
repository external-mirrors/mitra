use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::User,
    database::DatabaseError,
    profiles::types::{DbActor, DbActorProfile},
};

use crate::{
    authority::{Authority, AuthorityRoot},
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        compatible_id,
        local_activity_id_unified,
        local_actor_id_unified,
    },
    queues::OutgoingActivityJobData,
    vocabulary::UNDO,
};

use super::follow::{build_follow, Follow};

#[derive(Serialize)]
#[serde(untagged)]
enum FollowOrFollowId {
    Follow(Follow),
    Id(String),
}

#[derive(Serialize)]
struct UndoFollow {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: FollowOrFollowId,

    to: Vec<String>,
}

fn build_undo_follow(
    authority: &Authority,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: Uuid,
    follow_request_has_deprecated_ap_id: bool,
) -> UndoFollow {
    let follow = build_follow(
        authority,
        actor_profile,
        target_actor_id,
        follow_request_id,
        follow_request_has_deprecated_ap_id,
        false, // no context
    );
    let object = match authority.root() {
        AuthorityRoot::Server(_) => FollowOrFollowId::Follow(follow),
        // Unsigned embedded objects are not recommended
        AuthorityRoot::Key(_) => FollowOrFollowId::Id(follow.id),
    };
    let activity_id = local_activity_id_unified(
        authority,
        UNDO,
        follow_request_id,
    );
    let actor_id = local_actor_id_unified(
        authority,
        actor_profile.id,
        &actor_profile.username,
    );
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
    let authority = Authority::from(instance);
    let target_actor_id = compatible_id(target_actor, &target_actor.id)?;
    let activity = build_undo_follow(
        &authority,
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
    use apx_sdk::core::{
        crypto::eddsa::generate_weak_ed25519_key,
        url::http_uri::HttpUri,
    };
    use serde_json::{json, to_value};
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_undo_follow() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let source = DbActorProfile::local_for_test("user");
        let target_actor_id = "https://test.remote/users/123";
        let follow_request_id = generate_ulid();
        let activity = build_undo_follow(
            &authority,
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
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let source = DbActorProfile::local_for_test("user");
        let target_actor_id = "https://test.remote/users/123";
        let follow_request_id = generate_ulid();
        let activity = build_undo_follow(
            &authority,
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

    #[test]
    fn test_build_undo_follow_fep_ef61() {
        let secret_key = generate_weak_ed25519_key();
        let server_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let authority = Authority::key_with_gateway(&secret_key, &server_uri);
        let source = DbActorProfile::local_for_test("user");
        let target_actor_id = "https://test.remote/users/123";
        let follow_request_id = generate_ulid();
        let activity = build_undo_follow(
            &authority,
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
            "id": format!("{}/activities/undo/{}", authority, follow_request_id),
            "type": "Undo",
            "actor": activity.actor,
            "object": format!("{}/activities/follow/{}", authority, follow_request_id),
            "to": [target_actor_id],
        });
        assert_eq!(value, expected_value);
    }
}
