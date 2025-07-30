use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    emojis::types::DbEmoji,
    posts::types::{Post, Visibility},
    profiles::types::DbActorProfile,
    reactions::types::Reaction,
    users::types::User,
};
use mitra_services::media::MediaServer;

use crate::{
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        compatible_post_object_id,
        compatible_profile_actor_id,
        local_activity_id,
        local_actor_id,
        local_object_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::{DISLIKE, EMOJI_REACT, LIKE},
};

use super::emoji::{build_emoji, Emoji};

#[derive(Serialize)]
struct Like {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tag: Vec<Emoji>,

    to: Vec<String>,
    cc: Vec<String>,
}

pub(super) fn local_like_activity_id(
    instance_url: &str,
    reaction_id: Uuid,
    reaction_has_deprecated_ap_id: bool,
) -> String {
    if reaction_has_deprecated_ap_id {
        local_object_id(instance_url, reaction_id)
    } else {
        local_activity_id(instance_url, LIKE, reaction_id)
    }
}

pub(super) fn get_like_audience(
    note_author_id: &str,
    _note_visibility: Visibility,
) -> (Vec<String>, Vec<String>) {
    let primary_audience = vec![note_author_id.to_string()];
    let secondary_audience = vec![];
    (primary_audience, secondary_audience)
}

#[allow(clippy::too_many_arguments)]
fn build_like(
    instance_url: &str,
    media_server: &MediaServer,
    actor_profile: &DbActorProfile,
    object_id: &str,
    reaction_id: Uuid,
    maybe_reaction_content: Option<String>,
    maybe_custom_emoji: Option<&DbEmoji>,
    post_author_id: &str,
    post_visibility: Visibility,
    fep_c0e0_emoji_react_enabled: bool,
) -> Like {
    let activity_type = match maybe_reaction_content.as_deref() {
        Some("👎") => DISLIKE,
        Some(_) if fep_c0e0_emoji_react_enabled => EMOJI_REACT,
        Some(_) => LIKE,
        None => LIKE,
    };
    let activity_id = local_like_activity_id(instance_url, reaction_id, false);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    let maybe_tag = maybe_custom_emoji
        .map(|db_emoji| build_emoji(instance_url, media_server, db_emoji));
    let (primary_audience, secondary_audience) =
        get_like_audience(post_author_id, post_visibility);
    Like {
        context: build_default_context(),
        activity_type: activity_type.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id.to_string(),
        content: maybe_reaction_content,
        tag: maybe_tag.map(|tag| vec![tag]).unwrap_or_default(),
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn get_like_recipients(
    _db_client: &impl DatabaseClient,
    _instance_url: &str,
    post: &Post,
) -> Result<Vec<Recipient>, DatabaseError> {
    let mut recipients = vec![];
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.extend(Recipient::from_actor_data(remote_actor));
    };
    Ok(recipients)
}

pub async fn prepare_like(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    media_server: &MediaServer,
    sender: &User,
    post: &Post,
    reaction: &Reaction,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let recipients = get_like_recipients(
        db_client,
        &instance.url(),
        post,
    ).await?;
    let object_id = compatible_post_object_id(&instance.url(), post);
    let post_author_id =
        compatible_profile_actor_id(&instance.url(), &post.author);
    let activity = build_like(
        &instance.url(),
        media_server,
        &sender.profile,
        &object_id,
        reaction.id,
        reaction.content.clone(),
        reaction.emoji.as_ref(),
        &post_author_id,
        post.visibility,
        instance.federation.fep_c0e0_emoji_react_enabled,
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
    fn test_build_like() {
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let author = DbActorProfile::default();
        let post_id = "https://example.com/objects/123";
        let post_author_id = "https://example.com/users/test";
        let reaction_id = generate_ulid();
        let activity = build_like(
            INSTANCE_URL,
            &media_server,
            &author,
            post_id,
            reaction_id,
            None,
            None,
            post_author_id,
            Visibility::Public,
            false,
        );
        assert_eq!(
            activity.id,
            format!("{}/activities/like/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(activity.activity_type, "Like");
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.content.is_none(), true);
        assert_eq!(activity.tag.is_empty(), true);
        assert_eq!(activity.to, vec![post_author_id]);
        assert_eq!(activity.cc.is_empty(), true);
    }
}
