use apx_sdk::{
    core::url::canonical::NonCanonicalUri,
    constants::AP_PUBLIC,
};
use serde::Serialize;
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_models::{
    accounts::types::AutomatedAccountDetailed,
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
};
use mitra_utils::id::generate_ulid;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    forwarder::get_activity_recipients,
    identifiers::{
        local_activity_id_canonical,
        local_actor_id_canonical,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::ANNOUNCE,
};

#[derive(Serialize)]
struct GroupAnnounce {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: NonCanonicalUri,
    actor: NonCanonicalUri,
    object: JsonValue,

    audience: NonCanonicalUri,
    to: Vec<NonCanonicalUri>,
    cc: Vec<NonCanonicalUri>,
}

fn build_group_announce(
    authority: &Authority,
    group: &DbActorProfile,
    group_activity: JsonValue,
) -> GroupAnnounce {
    let id_builder = authority.id_builder();
    let canonical_actor_id = local_actor_id_canonical(
        authority.root(),
        group.id,
        &group.username,
    );
    let canonical_activity_id = local_activity_id_canonical(
        authority.root(),
        ANNOUNCE,
        generate_ulid(),
    );
    let canonical_followers =
        LocalActorCollection::Followers.of(&canonical_actor_id.to_string());
    GroupAnnounce {
        _context: build_default_context(),
        activity_type: ANNOUNCE.to_owned(),
        id: id_builder.build(&canonical_activity_id),
        actor: id_builder.build(&canonical_actor_id),
        object: group_activity,
        audience: id_builder.build(&canonical_actor_id),
        to: vec![id_builder.build_unchecked(AP_PUBLIC)],
        cc: vec![id_builder.build_unchecked(&canonical_followers)],
    }
}

pub async fn prepare_group_announce(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &AutomatedAccountDetailed,
    group_activity: JsonValue,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let group_announce = build_group_announce(
        &authority,
        &sender.profile,
        group_activity,
    );
    let audience: Vec<_> = group_announce
        .to
        .iter()
        .chain(&group_announce.cc)
        .map(|id| id.clone().into_canonical())
        .collect();
    let recipients = get_activity_recipients(
        db_client,
        &audience,
    ).await?;
    let recipients = recipients
        .into_iter()
        .filter_map(|profile| profile.actor_json)
        .flat_map(|actor_data| Recipient::for_inbox(&actor_data))
        .collect();
    Ok(OutgoingActivityJobData::new(
        &authority,
        sender,
        group_announce,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use mitra_models::{
        profiles::types::DbActorProfile,
    };
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";

    #[test]
    fn test_build_group_announce() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let group = DbActorProfile::local_for_test("group");
        let group_activity = json!({"type": "Create"});
        let activity = build_group_announce(
            &authority,
            &group,
            group_activity.clone(),
        );
        assert_eq!(activity.activity_type, "Announce");
        assert!(
            activity.id.to_string()
                .starts_with("https://social.example/activities/announce")
        );
        assert_eq!(
            activity.actor.to_string(),
            "https://social.example/users/group",
        );
        assert_eq!(activity.object, group_activity);
        assert_eq!(activity.audience, activity.actor);
        assert_eq!(activity.to[0].to_string(), AP_PUBLIC);
        assert_eq!(
            activity.cc[0].to_string(),
            "https://social.example/users/group/followers",
        );
    }
}
