use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::User,
    database::DatabaseError,
    profiles::types::{DbActor, DbActorProfile},
};
use mitra_utils::id::generate_ulid;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        compatible_id,
        local_activity_id_unified,
        local_actor_id_unified,
    },
    queues::OutgoingActivityJobData,
    vocabulary::ACCEPT,
};

#[derive(Serialize)]
struct AcceptFollow {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
}

fn build_accept_follow(
    authority: &Authority,
    actor_profile: &DbActorProfile,
    source_actor_id: &str,
    follow_activity_id: &str,
) -> AcceptFollow {
    // Accept(Follow) is idempotent so its ID can be random
    let activity_id = local_activity_id_unified(authority, ACCEPT, generate_ulid());
    let actor_id = local_actor_id_unified(
        authority,
        actor_profile.id,
        &actor_profile.username,
    );
    AcceptFollow {
        _context: build_default_context(),
        activity_type: ACCEPT.to_string(),
        id: activity_id,
        actor: actor_id,
        object: follow_activity_id.to_string(),
        to: vec![source_actor_id.to_string()],
    }
}

pub fn prepare_accept_follow(
    instance: &Instance,
    sender: &User,
    source_actor: &DbActor,
    follow_activity_id: &str,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let source_actor_id = compatible_id(source_actor, &source_actor.id)?;
    let follow_activity_id = compatible_id(
        source_actor,
        follow_activity_id,
    )?;
    let activity = build_accept_follow(
        &authority,
        &sender.profile,
        &source_actor_id,
        &follow_activity_id,
    );
    let recipients = Recipient::for_inbox(source_actor);
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_accept_follow() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let target = DbActorProfile::local_for_test("user");
        let follow_activity_id = "https://remote.example/objects/999";
        let follower_id = "https://remote.example/users/123";
        let activity = build_accept_follow(
            &authority,
            &target,
            follower_id,
            follow_activity_id,
        );

        assert_eq!(activity.id.starts_with(INSTANCE_URI), true);
        assert_eq!(activity.activity_type, "Accept");
        assert_eq!(activity.object, follow_activity_id);
        assert_eq!(activity.to, vec![follower_id]);
    }
}
