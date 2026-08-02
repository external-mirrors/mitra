use apx_sdk::{
    constants::AP_PUBLIC,
    core::url::canonical::NonCanonicalUri,
};
use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::User,
    database::{DatabaseClient, DatabaseError},
    posts::{
        helpers::add_related_posts,
        types::PostDetailed,
    },
    profiles::types::DbActorProfile,
};
use mitra_services::media::MediaServer;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    forwarder::Deliverable,
    identifiers::{
        local_activity_id_canonical,
        local_activity_id_unified,
        local_actor_id_canonical,
        local_actor_id_unified,
        local_object_id_unified,
        post_object_id,
        profile_actor_id,
        IdBuilder,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::{DELETE, NOTE, TOMBSTONE},
};

use super::note::{build_note, get_note_recipients, Note};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tombstone {
    id: String,

    #[serde(rename = "type")]
    object_type: String,

    attributed_to: String,

    former_type: String,
}

#[derive(Serialize)]
struct DeleteNote {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Tombstone,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_delete_note(
    authority: &Authority,
    instance_webfinger_hostname: &str,
    media_server: &MediaServer,
    post: &PostDetailed,
) -> DeleteNote {
    assert!(post.is_local());
    let object_id = local_object_id_unified(authority, post.id);
    let activity_id = local_activity_id_unified(
        authority,
        DELETE,
        post.id,
    );
    let actor_id = local_actor_id_unified(
        authority,
        post.author.id,
        &post.author.username,
    );
    let Note { to, cc, .. } = build_note(
        instance_webfinger_hostname,
        authority,
        media_server,
        post,
        false,
    );
    DeleteNote {
        _context: build_default_context(),
        activity_type: DELETE.to_string(),
        id: activity_id,
        actor: actor_id.clone(),
        object: Tombstone {
            id: object_id,
            object_type: TOMBSTONE.to_string(),
            attributed_to: actor_id,
            former_type: NOTE.to_string(),
        },
        to: to,
        cc: cc,
    }
}

pub async fn prepare_delete_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    media_server: &MediaServer,
    author: &User,
    post: &PostDetailed,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let authority = Authority::from(instance);
    let mut post = post.clone();
    add_related_posts(db_client, vec![&mut post]).await?;
    let activity = build_delete_note(
        &authority,
        &instance.webfinger_hostname(),
        media_server,
        &post,
    );
    let recipients = get_note_recipients(db_client, &post).await?;
    Ok(OutgoingActivityJobData::new(
        &authority,
        author,
        activity,
        recipients,
    ))
}

fn public() -> NonCanonicalUri {
    NonCanonicalUri::parse(AP_PUBLIC).expect("Public URI should be valid")
}

#[derive(Serialize)]
struct DeleteGroupNote {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: NonCanonicalUri,
    actor: NonCanonicalUri,
    object: NonCanonicalUri,

    audience: NonCanonicalUri,
    to: Vec<NonCanonicalUri>,
    cc: Vec<NonCanonicalUri>,
}

fn build_delete_group_note(
    authority: &Authority,
    moderator: &DbActorProfile,
    post: &PostDetailed,
) -> DeleteGroupNote {
    let activity_id = local_activity_id_canonical(
        authority.root(),
        DELETE,
        post.id,
    );
    let moderator_id = local_actor_id_canonical(
        authority.root(),
        moderator.id,
        &moderator.username,
    );
    let moderator_id_builder = authority.id_builder();
    let post_id = post_object_id(authority, post);
    let post_author_id = profile_actor_id(authority, &post.author);
    let post_id_builder = IdBuilder::for_profile(authority, &post.author);
    let mut primary_audience = vec![
        public(), // Lemmy requires Public in `to`
        post_id_builder.build_unchecked(&post_author_id),
    ];
    let secondary_audience = vec![];
    let group = post.group.as_ref().expect("post should belong to group");
    let (group_id, maybe_group_followers) = match group.actor_json {
        Some(ref actor_data) => (
            actor_data.id.clone(),
            actor_data.followers.clone(),
        ),
        None => {
            let group_id = local_actor_id_unified(
                authority,
                group.id,
                &group.username,
            );
            let group_followers =
                LocalActorCollection::Followers.of(&group_id);
            (group_id, Some(group_followers))
        },
    };
    let group_id_builder = IdBuilder::for_profile(authority, group);
    primary_audience
        .push(group_id_builder.build_unchecked(&group_id));
    if let Some(followers) = maybe_group_followers {
        primary_audience
            .push(group_id_builder.build_unchecked(&followers));
    };
    DeleteGroupNote {
        _context: build_default_context(),
        activity_type: DELETE.to_owned(),
        id: moderator_id_builder.build(&activity_id),
        actor: moderator_id_builder.build(&moderator_id),
        object: post_id_builder.build_unchecked(&post_id),
        audience: group_id_builder.build_unchecked(&group_id),
        to: primary_audience,
        cc: secondary_audience,
    }
}

impl Deliverable for DeleteGroupNote {
    fn to(&self) -> &[NonCanonicalUri] { &self.to }
    fn cc(&self) -> &[NonCanonicalUri] { &self.cc }
}

pub async fn prepare_delete_group_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    moderator: &User,
    post: &PostDetailed,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let delete_note = build_delete_group_note(
        &authority,
        &moderator.profile,
        post,
    );
    let recipients = delete_note.get_recipients(db_client).await?;
    Ok(OutgoingActivityJobData::new(
        &authority,
        moderator,
        delete_note,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use apx_sdk::{
        constants::AP_PUBLIC,
        core::url::http_uri::HttpUri,
    };
    use serde_json::json;
    use uuid::uuid;
    use mitra_models::{
        posts::types::RelatedPosts,
        profiles::types::DbActorProfile,
    };
    use super::*;

    const INSTANCE_URI: &str = "https://social.example";
    const INSTANCE_HOSTNAME: &str = "social.example";

    #[test]
    fn test_build_delete_note() {
        let instance_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let authority = Authority::server(&instance_uri);
        let media_server = MediaServer::for_test(INSTANCE_URI);
        let author = DbActorProfile::local_for_test("author");
        let post = PostDetailed {
            id: uuid!("c9386582-c7c3-4e90-8dde-4ab4b1943d96"),
            author,
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let activity = build_delete_note(
            &authority,
            INSTANCE_HOSTNAME,
            &media_server,
            &post,
        );
        let activity_value = serde_json::to_value(activity).unwrap();
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
            "id": "https://social.example/activities/delete/c9386582-c7c3-4e90-8dde-4ab4b1943d96",
            "type": "Delete",
            "actor": "https://social.example/users/author",
            "object": {
                "id": "https://social.example/objects/c9386582-c7c3-4e90-8dde-4ab4b1943d96",
                "type": "Tombstone",
                "attributedTo": "https://social.example/users/author",
                "formerType": "Note",
            },
            "to": [AP_PUBLIC],
            "cc": ["https://social.example/users/author/followers"],
        });
        assert_eq!(activity_value, expected_value);
    }

    #[test]
    fn test_build_delete_group_note() {
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let author = DbActorProfile::local_for_test("author");
        let group = DbActorProfile::local_for_test("group");
        let moderator = DbActorProfile::local_for_test("moderator");
        let post = PostDetailed {
            id: uuid!("c9386582-c7c3-4e90-8dde-4ab4b1943d96"),
            author,
            group: Some(group),
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let activity = build_delete_group_note(
            &authority,
            &moderator,
            &post,
        );
        let activity_value = serde_json::to_value(activity).unwrap();
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
            "id": "https://social.example/activities/delete/c9386582-c7c3-4e90-8dde-4ab4b1943d96",
            "type": "Delete",
            "actor": "https://social.example/users/moderator",
            "object": "https://social.example/objects/c9386582-c7c3-4e90-8dde-4ab4b1943d96",
            "audience": "https://social.example/users/group",
            "to": [
                AP_PUBLIC,
                "https://social.example/users/author",
                "https://social.example/users/group",
                "https://social.example/users/group/followers",
            ],
            "cc": [],
        });
        assert_eq!(activity_value, expected_value);
    }
}
