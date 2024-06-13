use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_federation::constants::AP_PUBLIC;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::Post,
    profiles::types::DbActorProfile,
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{
        local_activity_id,
        local_actor_id,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::UNDO,
};
use super::announce::{
    get_announce_recipients,
    local_announce_activity_id,
};

#[derive(Serialize)]
struct UndoAnnounce {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_undo_announce(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    repost_id: Uuid,
    repost_has_deprecated_ap_id: bool,
    recipient_id: &str,
) -> UndoAnnounce {
    let object_id = local_announce_activity_id(
        instance_url,
        repost_id,
        repost_has_deprecated_ap_id,
    );
    let activity_id = local_activity_id(instance_url, UNDO, repost_id);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    let primary_audience = vec![
        AP_PUBLIC.to_string(),
        recipient_id.to_string(),
    ];
    let secondary_audience = vec![
        LocalActorCollection::Followers.of(&actor_id),
    ];
    UndoAnnounce {
        context: build_default_context(),
        activity_type: UNDO.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn prepare_undo_announce(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    post: &Post,
    repost_id: Uuid,
    repost_has_deprecated_ap_id: bool,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_ne!(post.id, repost_id);
    let (recipients, primary_recipient) = get_announce_recipients(
        db_client,
        &instance.url(),
        sender,
        post,
    ).await?;
    let activity = build_undo_announce(
        &instance.url(),
        &sender.profile,
        repost_id,
        repost_has_deprecated_ap_id,
        &primary_recipient,
    );
    Ok(OutgoingActivityJobData::new(
        &instance.url(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_undo_announce() {
        let announcer = DbActorProfile::default();
        let post_author_id = "https://example.com/users/test";
        let repost_id = generate_ulid();
        let activity = build_undo_announce(
            INSTANCE_URL,
            &announcer,
            repost_id,
            true, // legacy activity ID
            post_author_id,
        );
        assert_eq!(
            activity.id,
            format!("{}/activities/undo/{}", INSTANCE_URL, repost_id),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URL, repost_id),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC, post_author_id]);
        assert_eq!(activity.cc, vec![
            format!("{}/users/{}/followers", INSTANCE_URL, announcer.username),
        ]);

        let activity = build_undo_announce(
            INSTANCE_URL,
            &announcer,
            repost_id,
            false, // no legacy activity ID
            post_author_id,
        );
        assert_eq!(
            activity.object,
            format!("{}/activities/announce/{}", INSTANCE_URL, repost_id),
        );
    }
}
