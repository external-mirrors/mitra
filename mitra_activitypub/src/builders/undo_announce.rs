use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::{Post, Repost, Visibility},
    profiles::types::DbActorProfile,
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{
        local_activity_id,
        local_actor_id,
        profile_actor_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::UNDO,
};
use super::announce::{
    get_announce_audience,
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
    instance_uri: &str,
    actor_profile: &DbActorProfile,
    repost_id: Uuid,
    repost_has_deprecated_ap_id: bool,
    repost_visibility: Visibility,
    post_author: &DbActorProfile,
) -> UndoAnnounce {
    let object_id = local_announce_activity_id(
        instance_uri,
        repost_id,
        repost_has_deprecated_ap_id,
    );
    let activity_id = local_activity_id(instance_uri, UNDO, repost_id);
    let actor_id = local_actor_id(instance_uri, &actor_profile.username);
    let recipient_id = profile_actor_id(instance_uri, post_author);
    let (primary_audience, secondary_audience) = get_announce_audience(
        repost_visibility,
        &actor_id,
        &recipient_id,
    );
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
    repost: &Repost,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_ne!(post.id, repost.id);
    let recipients = get_announce_recipients(
        db_client,
        sender,
        repost.visibility,
        post,
    ).await?;
    let activity = build_undo_announce(
        instance.uri_str(),
        &sender.profile,
        repost.id,
        repost.has_deprecated_ap_id,
        repost.visibility,
        &post.author,
    );
    Ok(OutgoingActivityJobData::new(
        instance.uri_str(),
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use apx_sdk::constants::AP_PUBLIC;
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_undo_announce() {
        let announcer = DbActorProfile::default();
        let post_author_id = "https://social.example/users/test";
        let post_author = DbActorProfile::remote_for_test(
            "author",
            post_author_id,
        );
        let repost_id = generate_ulid();
        let activity = build_undo_announce(
            INSTANCE_URI,
            &announcer,
            repost_id,
            true, // legacy activity ID
            Visibility::Public,
            &post_author,
        );
        assert_eq!(
            activity.id,
            format!("{}/activities/undo/{}", INSTANCE_URI, repost_id),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URI, repost_id),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC, post_author_id]);
        assert_eq!(activity.cc, vec![
            format!("{}/users/{}/followers", INSTANCE_URI, announcer.username),
        ]);

        let activity = build_undo_announce(
            INSTANCE_URI,
            &announcer,
            repost_id,
            false, // no legacy activity ID
            Visibility::Public,
            &post_author,
        );
        assert_eq!(
            activity.object,
            format!("{}/activities/announce/{}", INSTANCE_URI, repost_id),
        );
    }
}
