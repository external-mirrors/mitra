use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_federation::{
    addresses::ActorAddress,
    constants::{AP_MEDIA_TYPE, AP_PUBLIC},
    deserialization::deserialize_string_array,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::get_post_author,
    posts::types::{Post, Visibility},
    profiles::types::DbActor,
    relationships::queries::{get_followers, get_subscribers},
    users::types::User,
};
use mitra_services::media::get_file_url;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    identifiers::{
        local_actor_id_unified,
        local_object_id_unified,
        local_object_replies,
        local_tag_collection,
        post_object_id,
        profile_actor_id,
        LocalActorCollection,
    },
    vocabulary::{DOCUMENT, HASHTAG, LINK, MENTION, NOTE},
};

use super::emoji::{build_emoji, Emoji};

const LINK_REL_MISSKEY_QUOTE: &str = "https://misskey-hub.net/ns#_misskey_quote";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SimpleTag {
    #[serde(rename = "type")]
    tag_type: String,
    href: String,
    name: String,
}

/// https://codeberg.org/silverpill/feps/src/branch/main/e232/fep-e232.md
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub name: Option<String>,
    pub href: String,
    pub media_type: String,
    #[serde(
        default,
        deserialize_with = "deserialize_string_array",
        skip_serializing_if = "Vec::is_empty",
    )]
    pub rel: Vec<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Tag {
    SimpleTag(SimpleTag),
    LinkTag(LinkTag),
    EmojiTag(Emoji),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaAttachment {
    #[serde(rename = "type")]
    attachment_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    media_type: Option<String>,
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(rename = "@context", skip_serializing_if = "Option::is_none")]
    pub(super) _context: Option<Context>,

    pub id: String,

    #[serde(rename = "type")]
    object_type: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachment: Vec<MediaAttachment>,

    pub(super) attributed_to: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    in_reply_to: Option<String>,

    replies: String,

    content: String,
    sensitive: bool,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    tag: Vec<Tag>,

    pub to: Vec<String>,
    pub cc: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    quote_url: Option<String>,

    published: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) updated: Option<DateTime<Utc>>,
}

pub fn build_note(
    instance_hostname: &str,
    instance_url: &str,
    authority: &Authority,
    post: &Post,
    fep_e232_enabled: bool,
    with_context: bool,
) -> Note {
    assert_eq!(authority.server_url(), instance_url);
    let object_id = local_object_id_unified(authority, post.id);
    let actor_id = local_actor_id_unified(authority, &post.author.username);
    let attachments: Vec<_> = post.attachments.iter().map(|db_item| {
        let url = get_file_url(instance_url, &db_item.file_name);
        MediaAttachment {
            attachment_type: DOCUMENT.to_string(),
            name: db_item.description.clone(),
            media_type: db_item.media_type.clone(),
            url,
        }
    }).collect();

    let mut primary_audience = vec![];
    let mut secondary_audience = vec![];
    let followers_collection_id =
        LocalActorCollection::Followers.of(&actor_id);
    let subscribers_collection_id =
        LocalActorCollection::Subscribers.of(&actor_id);
    match post.visibility {
        Visibility::Public => {
            primary_audience.push(AP_PUBLIC.to_string());
            secondary_audience.push(followers_collection_id);
        },
        Visibility::Followers => {
            primary_audience.push(followers_collection_id);
        },
        Visibility::Subscribers => {
            primary_audience.push(subscribers_collection_id);
        },
        Visibility::Direct => (),
    };

    let mut tags = vec![];
    for profile in &post.mentions {
        let actor_address = ActorAddress::new(
            &profile.username,
            profile.hostname.as_deref().unwrap_or(instance_hostname),
        );
        let tag_name = actor_address.handle();
        let actor_id = profile_actor_id(instance_url, profile);
        if !primary_audience.contains(&actor_id) {
            primary_audience.push(actor_id.clone());
        };
        let tag = SimpleTag {
            tag_type: MENTION.to_string(),
            name: tag_name,
            href: actor_id,
        };
        tags.push(Tag::SimpleTag(tag));
    };
    for tag_name in &post.tags {
        let tag_href = local_tag_collection(instance_url, tag_name);
        let tag = SimpleTag {
            tag_type: HASHTAG.to_string(),
            name: format!("#{}", tag_name),
            href: tag_href,
        };
        tags.push(Tag::SimpleTag(tag));
    };

    assert_eq!(post.links.len(), post.linked.len());
    for (index, linked) in post.linked.iter().enumerate() {
        // Build FEP-e232 object link
        // https://codeberg.org/silverpill/feps/src/branch/main/e232/fep-e232.md
        let link_href = post_object_id(instance_url, linked);
        let link_rel = if index == 0 {
            // Present first link as a quote
            vec![LINK_REL_MISSKEY_QUOTE.to_string()]
        } else {
            vec![]
        };
        let tag = LinkTag {
            tag_type: LINK.to_string(),
            name: None,  // no microsyntax
            href: link_href,
            media_type: AP_MEDIA_TYPE.to_string(),
            rel: link_rel,
        };
        if fep_e232_enabled {
            tags.push(Tag::LinkTag(tag));
        };
    };
    // Present first link as a quote
    let maybe_quote_url = post.linked.first()
        .map(|linked| post_object_id(instance_url, linked));

    for emoji in &post.emojis {
        let tag = build_emoji(instance_url, emoji);
        tags.push(Tag::EmojiTag(tag));
    };

    let in_reply_to_object_id = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let in_reply_to = post.in_reply_to.as_ref()
                .expect("in_reply_to should be populated");
            assert_eq!(in_reply_to.id, in_reply_to_id);
            let in_reply_to_actor_id = profile_actor_id(
                instance_url,
                &in_reply_to.author,
            );
            if !primary_audience.contains(&in_reply_to_actor_id) {
                primary_audience.push(in_reply_to_actor_id);
            };
            Some(post_object_id(instance_url, in_reply_to))
        },
        None => None,
    };
    let replies_collection_id = local_object_replies(&object_id);
    Note {
        _context: with_context.then(build_default_context),
        id: object_id,
        object_type: NOTE.to_string(),
        attachment: attachments,
        attributed_to: actor_id,
        in_reply_to: in_reply_to_object_id,
        replies: replies_collection_id,
        content: post.content.clone(),
        sensitive: post.is_sensitive,
        tag: tags,
        to: primary_audience,
        cc: secondary_audience,
        quote_url: maybe_quote_url,
        published: post.created_at,
        updated: post.updated_at,
    }
}

pub async fn get_note_recipients(
    db_client: &impl DatabaseClient,
    current_user: &User,
    post: &Post,
) -> Result<Vec<DbActor>, DatabaseError> {
    let mut audience = vec![];
    match post.visibility {
        Visibility::Public | Visibility::Followers => {
            let followers = get_followers(db_client, &current_user.id).await?;
            audience.extend(followers);
        },
        Visibility::Subscribers => {
            let subscribers = get_subscribers(db_client, &current_user.id).await?;
            audience.extend(subscribers);
        },
        Visibility::Direct => (),
    };
    if let Some(in_reply_to_id) = post.in_reply_to_id {
        // TODO: use post.in_reply_to ?
        let in_reply_to_author = get_post_author(db_client, in_reply_to_id).await?;
        audience.push(in_reply_to_author);
    };
    audience.extend(post.mentions.clone());

    let mut recipients = vec![];
    for profile in audience {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::uuid;
    use mitra_models::{
        profiles::types::DbActorProfile,
        users::types::User,
    };
    use super::*;

    const INSTANCE_HOSTNAME: &str = "server.example";
    const INSTANCE_URL: &str = "https://server.example";

    #[test]
    fn test_build_tag() {
        let simple_tag = SimpleTag {
            tag_type: HASHTAG.to_string(),
            href: "https://server.example/tags/test".to_string(),
            name: "#test".to_string(),
        };
        let tag = Tag::SimpleTag(simple_tag);
        let value = serde_json::to_value(tag).unwrap();
        assert_eq!(value, json!({
            "type": "Hashtag",
            "href": "https://server.example/tags/test",
            "name": "#test",
        }));
    }

    #[test]
    fn test_build_note() {
        let author = DbActorProfile {
            username: "author".to_string(),
            ..Default::default()
        };
        let post = Post {
            author,
            tags: vec!["test".to_string()],
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &post,
            false,
            true,
        );

        assert_eq!(note._context.is_some(), true);
        assert_eq!(
            note.id,
            format!("{}/objects/{}", INSTANCE_URL, post.id),
        );
        assert_eq!(note.attachment.len(), 0);
        assert_eq!(
            note.attributed_to,
            format!("{}/users/{}", INSTANCE_URL, post.author.username),
        );
        assert_eq!(note.in_reply_to.is_none(), true);
        assert_eq!(
            note.replies,
            format!("{}/objects/{}/replies", INSTANCE_URL, post.id),
        );
        assert_eq!(note.content, post.content);
        assert_eq!(note.to, vec![AP_PUBLIC]);
        assert_eq!(note.cc, vec![
            format!("{INSTANCE_URL}/users/author/followers"),
        ]);
        assert_eq!(note.tag.len(), 1);
        let tag = match note.tag[0] {
            Tag::SimpleTag(ref tag) => tag,
            _ => panic!(),
        };
        assert_eq!(tag.name, "#test");
        assert_eq!(tag.href, "https://server.example/collections/tags/test");

        assert_eq!(note.published, post.created_at);
        assert_eq!(note.updated, None);
    }

    #[test]
    fn test_build_note_followers_only() {
        let post = Post {
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &post,
            false,
            true,
        );

        assert_eq!(note.to, vec![
            format!("{}/users/{}/followers", INSTANCE_URL, post.author.username),
        ]);
        assert_eq!(note.cc.is_empty(), true);
    }

    #[test]
    fn test_build_note_subscribers_only() {
        let subscriber_id = "https://test.com/users/3";
        let subscriber = DbActorProfile {
            username: "subscriber".to_string(),
            hostname: Some("test.com".to_string()),
            actor_json: Some(DbActor {
                id: subscriber_id.to_string(),
                ..Default::default()
            }),
            actor_id: Some(subscriber_id.to_string()),
            ..Default::default()
        };
        let post = Post {
            visibility: Visibility::Subscribers,
            mentions: vec![subscriber],
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &post,
            false,
            true,
        );

        assert_eq!(note.to, vec![
            format!("{}/users/{}/subscribers", INSTANCE_URL, post.author.username),
            subscriber_id.to_string(),
        ]);
        assert_eq!(note.cc.is_empty(), true);
    }

    #[test]
    fn test_build_note_direct() {
        let mentioned_id = "https://test.com/users/3";
        let mentioned = DbActorProfile {
            username: "mention".to_string(),
            hostname: Some("test.com".to_string()),
            actor_json: Some(DbActor {
                id: mentioned_id.to_string(),
                ..Default::default()
            }),
            actor_id: Some(mentioned_id.to_string()),
            ..Default::default()
        };
        let post = Post {
            visibility: Visibility::Direct,
            mentions: vec![mentioned],
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &post,
            false,
            true,
        );

        assert_eq!(note.to, vec![mentioned_id]);
        assert_eq!(note.cc.is_empty(), true);
    }

    #[test]
    fn test_build_note_with_local_parent() {
        let parent = Post::default();
        let post = Post {
            in_reply_to_id: Some(parent.id),
            in_reply_to: Some(Box::new(parent.clone())),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &post,
            false,
            true,
        );

        assert_eq!(
            note.in_reply_to.unwrap(),
            format!("{}/objects/{}", INSTANCE_URL, parent.id),
        );
        assert_eq!(note.to, vec![
            AP_PUBLIC.to_string(),
            format!("{}/users/{}", INSTANCE_URL, parent.author.username),
        ]);
    }

    #[test]
    fn test_build_note_with_remote_parent() {
        let parent_author_acct = "test@test.net";
        let parent_author_actor_id = "https://test.net/user/test";
        let parent_author_actor_url = "https://test.net/@test";
        let parent_author = DbActorProfile {
            username: "test".to_string(),
            hostname: Some("test.net".to_string()),
            acct: parent_author_acct.to_string(),
            actor_json: Some(DbActor {
                id: parent_author_actor_id.to_string(),
                url: Some(parent_author_actor_url.to_string()),
                ..Default::default()
            }),
            actor_id: Some(parent_author_actor_id.to_string()),
            ..Default::default()
        };
        let parent = Post {
            author: parent_author.clone(),
            object_id: Some("https://test.net/obj/123".to_string()),
            ..Default::default()
        };
        let post = Post {
            in_reply_to_id: Some(parent.id),
            in_reply_to: Some(Box::new(parent.clone())),
            mentions: vec![parent_author],
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &post,
            false,
            true,
        );

        assert_eq!(
            note.in_reply_to.unwrap(),
            parent.object_id.unwrap(),
        );
        let tags = note.tag;
        assert_eq!(tags.len(), 1);
        let tag = match tags[0] {
            Tag::SimpleTag(ref tag) => tag,
            _ => panic!(),
        };
        assert_eq!(tag.name, format!("@{}", parent_author_acct));
        assert_eq!(tag.href, parent_author_actor_id);
        assert_eq!(note.to, vec![AP_PUBLIC, parent_author_actor_id]);
    }

    #[test]
    fn test_build_note_fep_ef61() {
        let author = User::default();
        let post = Post {
            id: uuid!("11fa64ff-b5a3-47bf-b23d-22b360581c3f"),
            author: author.profile.clone(),
            created_at: DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
                .unwrap().with_timezone(&Utc),
            ..Default::default()
        };
        let authority = Authority::from_user(INSTANCE_URL, &author, true);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &post,
            true,
            true,
        );
        let value = serde_json::to_value(note).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "mitra": "http://jsonld.mitra.social#",
                    "MitraJcsRsaSignature2022": "mitra:MitraJcsRsaSignature2022",
                    "proofValue": "sec:proofValue",
                    "proofPurpose": "sec:proofPurpose",
                    "verificationMethod": "sec:verificationMethod",
                },
            ],
            "id": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f",
            "type": "Note",
            "attributedTo": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "content": "",
            "sensitive": false,
            "replies": "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f/replies",
            "published": "2023-02-24T23:36:38Z",
            "to": [
                "https://www.w3.org/ns/activitystreams#Public",
            ],
            "cc": [
                "did:ap:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/followers",
            ],
        });
        assert_eq!(value, expected_value);
    }
}
