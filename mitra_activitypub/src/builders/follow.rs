use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::User,
    database::DatabaseError,
    profiles::types::{DbActor, DbActorProfile},
};

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        compatible_id,
        local_activity_id_unified,
        local_actor_id_unified,
        local_object_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::FOLLOW,
};

#[derive(Serialize)]
pub struct Follow {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub _context: Option<Context>,

    #[serde(rename = "type")]
    pub activity_type: String,

    pub id: String,
    pub actor: String,
    pub object: String,

    pub to: Vec<String>,
}

pub fn build_follow(
    authority: &Authority,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: Uuid,
    follow_request_has_deprecated_ap_id: bool,
    with_context: bool,
) -> Follow {
    let activity_id = if follow_request_has_deprecated_ap_id {
        let instance_uri = authority.expect_server_uri();
        local_object_id(instance_uri.as_str(), follow_request_id)
    } else {
        local_activity_id_unified(authority, FOLLOW, follow_request_id)
    };
    let actor_id = local_actor_id_unified(
        authority,
        actor_profile.id,
        &actor_profile.username,
    );
    Follow {
        _context: with_context.then(build_default_context),
        activity_type: FOLLOW.to_string(),
        id: activity_id,
        actor: actor_id,
        object: target_actor_id.to_string(),
        to: vec![target_actor_id.to_string()],
    }
}

pub fn prepare_follow(
    instance: &Instance,
    sender: &User,
    target_actor: &DbActor,
    follow_request_id: Uuid,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let target_actor_id = compatible_id(target_actor, &target_actor.id)?;
    let activity = build_follow(
        &authority,
        &sender.profile,
        &target_actor_id,
        follow_request_id,
        false, // don't use legacy activity IDs
        true, // with context
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
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_follow() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let follower = DbActorProfile::local_for_test("follower");
        let follow_request_id = generate_ulid();
        let target_actor_id = "https://test.remote/actor/test";
        let activity = build_follow(
            &authority,
            &follower,
            target_actor_id,
            follow_request_id,
            false, // don't use legacy activity IDs
            true, // with context
        );

        assert_eq!(activity._context.is_some(), true);
        assert_eq!(
            activity.id,
            format!("{}/activities/follow/{}", INSTANCE_URI, follow_request_id),
        );
        assert_eq!(activity.activity_type, "Follow");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URI, follower.username),
        );
        assert_eq!(activity.object, target_actor_id);
        assert_eq!(activity.to, vec![target_actor_id]);
    }
}
