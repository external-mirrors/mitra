use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::{Post, Visibility},
    profiles::types::DbActorProfile,
    users::types::User,
};

use crate::{
    contexts::{build_default_context, Context},
    identifiers::{
        compatible_profile_actor_id,
        local_activity_id,
        local_actor_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::UNDO,
};

use super::like::{
    get_like_audience,
    get_like_recipients,
    local_like_activity_id,
};

#[derive(Serialize)]
struct UndoLike {
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

fn build_undo_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    reaction_id: Uuid,
    reaction_has_deprecated_ap_id: bool,
    post_author_id: &str,
    post_visibility: &Visibility,
) -> UndoLike {
    let object_id = local_like_activity_id(
        instance_url,
        reaction_id,
        reaction_has_deprecated_ap_id,
    );
    let activity_id = local_activity_id(instance_url, UNDO, reaction_id);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    let (primary_audience, secondary_audience) =
        get_like_audience(post_author_id, post_visibility);
    UndoLike {
        context: build_default_context(),
        activity_type: UNDO.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn prepare_undo_like(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    post: &Post,
    reaction_id: Uuid,
    reaction_has_deprecated_ap_id: bool,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let recipients = get_like_recipients(
        db_client,
        &instance.url(),
        post,
    ).await?;
    let post_author_id =
        compatible_profile_actor_id(&instance.url(), &post.author);
    let activity = build_undo_like(
        &instance.url(),
        &sender.profile,
        reaction_id,
        reaction_has_deprecated_ap_id,
        &post_author_id,
        &post.visibility,
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
    use mitra_federation::constants::AP_PUBLIC;
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_undo_like() {
        let author = DbActorProfile::default();
        let post_author_id = "https://example.com/users/test";
        let reaction_id = generate_ulid();
        let activity = build_undo_like(
            INSTANCE_URL,
            &author,
            reaction_id,
            true, // legacy activity ID
            post_author_id,
            &Visibility::Public,
        );
        assert_eq!(
            activity.id,
            format!("{}/activities/undo/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(activity.to, vec![post_author_id, AP_PUBLIC]);
        assert_eq!(activity.cc.is_empty(), true);

        let activity = build_undo_like(
            INSTANCE_URL,
            &author,
            reaction_id,
            false, // no legacy activity ID
            post_author_id,
            &Visibility::Public,
        );
        assert_eq!(
            activity.object,
            format!("{}/activities/like/{}", INSTANCE_URL, reaction_id),
        );
    }
}
