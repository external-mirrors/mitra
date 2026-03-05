use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    emojis::types::{CustomEmoji as DbCustomEmoji},
    posts::types::{PostDetailed, Visibility},
    profiles::types::DbActorProfile,
    reactions::types::ReactionDetailed,
    users::types::User,
};
use mitra_services::media::MediaServer;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        compatible_post_object_id,
        compatible_profile_actor_id,
        local_activity_id_unified,
        local_actor_id_unified,
        local_object_id,
    },
    queues::OutgoingActivityJobData,
    vocabulary::{DISLIKE, EMOJI_REACT, LIKE},
};

use super::emoji::{build_emoji, Emoji};

#[derive(Serialize)]
pub struct Like {
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
    authority: &Authority,
    reaction_id: Uuid,
    reaction_has_deprecated_ap_id: bool,
) -> String {
    if reaction_has_deprecated_ap_id {
        let instance_uri = authority.expect_server_uri();
        local_object_id(instance_uri, reaction_id)
    } else {
        local_activity_id_unified(authority, LIKE, reaction_id)
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
pub fn build_like(
    authority: &Authority,
    media_server: &MediaServer,
    actor_profile: &DbActorProfile,
    post: &PostDetailed,
    reaction_id: Uuid,
    maybe_reaction_content: Option<String>,
    maybe_custom_emoji: Option<&DbCustomEmoji>,
    fep_c0e0_emoji_react_enabled: bool,
) -> Like {
    let activity_type = match maybe_reaction_content.as_deref() {
        Some("👎") => DISLIKE,
        Some(_) if fep_c0e0_emoji_react_enabled => EMOJI_REACT,
        Some(_) => LIKE,
        None => LIKE,
    };
    let activity_id = local_like_activity_id(authority, reaction_id, false);
    let actor_id = local_actor_id_unified(
        authority,
        actor_profile.id,
        &actor_profile.username,
    );
    let object_id = compatible_post_object_id(authority, post);
    let instance_uri = authority.expect_server_uri();
    let maybe_tag = maybe_custom_emoji
        .map(|db_emoji| build_emoji(instance_uri, media_server, db_emoji));
    let post_author_id =
        compatible_profile_actor_id(authority, &post.author);
    let (primary_audience, secondary_audience) =
        get_like_audience(&post_author_id, post.visibility);
    Like {
        context: build_default_context(),
        activity_type: activity_type.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id,
        content: maybe_reaction_content,
        tag: maybe_tag.map(|tag| vec![tag]).unwrap_or_default(),
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn get_like_recipients(
    _db_client: &impl DatabaseClient,
    _instance_uri: &str,
    post: &PostDetailed,
) -> Result<Vec<Recipient>, DatabaseError> {
    let mut recipients = vec![];
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.extend(Recipient::for_inbox(remote_actor));
    };
    Ok(recipients)
}

pub async fn prepare_like(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    media_server: &MediaServer,
    sender: &User,
    post: &PostDetailed,
    reaction: &ReactionDetailed,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let recipients = get_like_recipients(
        db_client,
        instance.uri_str(),
        post,
    ).await?;
    let authority = Authority::from(instance);
    let activity = build_like(
        &authority,
        media_server,
        &sender.profile,
        post,
        reaction.id,
        reaction.content.clone(),
        reaction.emoji.as_ref(),
        instance.federation.fep_c0e0_emoji_react_enabled,
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
    use apx_sdk::core::{
        crypto::eddsa::generate_weak_ed25519_key,
        url::http_uri::HttpUri,
    };
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_like() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let media_server = MediaServer::for_test(INSTANCE_URI);
        let author = DbActorProfile::default();
        let post_id = "https://example.com/objects/123";
        let post_author_id = "https://example.com/users/test";
        let post_author = DbActorProfile::remote_for_test("test", post_author_id);
        let post = PostDetailed::remote_for_test(&post_author, post_id);
        let reaction_id = generate_ulid();
        let activity = build_like(
            &authority,
            &media_server,
            &author,
            &post,
            reaction_id,
            None,
            None,
            false,
        );
        assert_eq!(
            activity.id,
            format!("{}/activities/like/{}", INSTANCE_URI, reaction_id),
        );
        assert_eq!(activity.activity_type, "Like");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URI, author.username),
        );
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.content.is_none(), true);
        assert_eq!(activity.tag.is_empty(), true);
        assert_eq!(activity.to, vec![post_author_id]);
        assert_eq!(activity.cc.is_empty(), true);
    }

    #[test]
    fn test_build_like_fep_ef61() {
        let secret_key = generate_weak_ed25519_key();
        let server_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let authority = Authority::key_with_gateway(&secret_key, &server_uri);
        let media_server = MediaServer::for_test(INSTANCE_URI);
        let author = DbActorProfile::local_for_test("portable");
        let post_id = "https://example.com/objects/123";
        let post_author_id = "https://example.com/users/test";
        let post_author = DbActorProfile::remote_for_test("test", post_author_id);
        let post = PostDetailed::remote_for_test(&post_author, post_id);
        let reaction_id = generate_ulid();
        let activity = build_like(
            &authority,
            &media_server,
            &author,
            &post,
            reaction_id,
            None,
            None,
            false,
        );
        assert_eq!(
            activity.id,
            format!("{}/activities/like/{}", authority, reaction_id),
        );
        assert_eq!(
            activity.actor,
            format!("{}/actors/{}", authority, author.id),
        );
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.to, vec![post_author_id]);
    }
}
