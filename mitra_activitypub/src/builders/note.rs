use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use apx_sdk::{
    addresses::WebfingerAddress,
    constants::{AP_MEDIA_TYPE, AP_PUBLIC},
    core::hashes::sha256_multibase,
    deserialization::deserialize_string_array,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    attachments::types::AttachmentType,
    polls::queries::get_voters,
    posts::{
        queries::get_post_author,
        types::{Post, Visibility},
    },
    profiles::types::WebfingerHostname,
    relationships::queries::{get_followers, get_subscribers},
};
use mitra_services::media::MediaServer;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        compatible_id,
        compatible_post_object_id,
        compatible_profile_actor_id,
        local_actor_id,
        local_actor_id_unified,
        local_conversation_collection,
        local_object_id_unified,
        local_object_replies,
        local_tag_collection,
        LocalActorCollection,
    },
    vocabulary::{
        DOCUMENT,
        HASHTAG,
        IMAGE,
        LINK,
        MENTION,
        NOTE,
        QUESTION,
    },
};

use super::emoji::{build_emoji, Emoji};

const LINK_REL_MISSKEY_QUOTE: &str = "https://misskey-hub.net/ns#_misskey_quote";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuestionReplies {
    total_items: u32,
}

#[derive(Serialize)]
struct QuestionOption {
    #[serde(rename = "type")]
    object_type: String,
    name: String,
    replies: QuestionReplies,
}

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
    digest_multibase: Option<String>,
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

    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,

    replies: String,

    content: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    content_map: Option<HashMap<String, String>>,

    sensitive: bool,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    tag: Vec<Tag>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    one_of: Vec<QuestionOption>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    any_of: Vec<QuestionOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time: Option<DateTime<Utc>>,

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
    media_server: &MediaServer,
    post: &Post,
    fep_e232_enabled: bool,
    with_context: bool,
) -> Note {
    let related_posts = post.expect_related_posts();
    assert_eq!(authority.server_url(), instance_url);
    let object_id = local_object_id_unified(authority, post.id);
    let mut object_type = NOTE;
    let actor_id = local_actor_id_unified(authority, &post.author.username);
    let attachments: Vec<_> = post.attachments.iter().map(|db_item| {
        let url = media_server.url_for(&db_item.file_name);
        let object_type = match db_item.attachment_type() {
            AttachmentType::Image => IMAGE,
            _ => DOCUMENT,
        };
        MediaAttachment {
            attachment_type: object_type.to_string(),
            name: db_item.description.clone(),
            media_type: db_item.media_type.clone(),
            digest_multibase: db_item.digest.as_ref()
                .map(|digest| sha256_multibase(digest)),
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
        Visibility::Conversation => (),
        Visibility::Direct => (),
    };

    let (one_of, any_of, end_time) = if let Some(ref poll) = post.poll {
        object_type = QUESTION;
        let results = poll.results.inner().iter()
            .map(|result| {
                QuestionOption {
                    object_type: NOTE.to_string(),
                    name: result.option_name.clone(),
                    replies: QuestionReplies {
                        total_items: result.vote_count,
                    },
                }
            })
            .collect();
        if poll.multiple_choices {
            (vec![], results, Some(poll.ends_at))
        } else {
            (results, vec![], Some(poll.ends_at))
        }
    } else {
        (vec![], vec![], None)
    };

    let mut tags = vec![];
    for profile in &post.mentions {
        let tag_name = match profile.hostname() {
            WebfingerHostname::Local => {
                WebfingerAddress::new_unchecked(
                    &profile.username, instance_hostname).handle()
            },
            WebfingerHostname::Remote(hostname) => {
                WebfingerAddress::new_unchecked(
                    &profile.username, &hostname).handle()
            },
            WebfingerHostname::Unknown => format!("@{}", profile.username),
        };
        let actor_id = compatible_profile_actor_id(instance_url, profile);
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

    for (index, linked) in related_posts.linked.iter().enumerate() {
        // Build FEP-e232 object link
        // https://codeberg.org/silverpill/feps/src/branch/main/e232/fep-e232.md
        let link_href = compatible_post_object_id(instance_url, linked);
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
    let maybe_quote_url = related_posts
        .linked.first()
        .map(|linked| compatible_post_object_id(instance_url, linked));

    for emoji in &post.emojis {
        let tag = build_emoji(instance_url, media_server, emoji);
        tags.push(Tag::EmojiTag(tag));
    };

    let in_reply_to_object_id = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let in_reply_to = related_posts
                .in_reply_to.as_ref()
                .expect("in_reply_to should be populated");
            assert_eq!(in_reply_to.id, in_reply_to_id);
            if post.author.id != in_reply_to.author.id {
                // Add author of a parent post to audience
                let in_reply_to_actor_id = compatible_profile_actor_id(
                    instance_url,
                    &in_reply_to.author,
                );
                if !primary_audience.contains(&in_reply_to_actor_id) {
                    primary_audience.push(in_reply_to_actor_id);
                };
            };
            if post.visibility == Visibility::Conversation {
                // Copy conversation audience
                let conversation = in_reply_to.expect_conversation();
                // Public conversations have empty audience.
                // Conversations created by database migration
                // will also have empty audience.
                if let Some(ref audience) = conversation.audience {
                    if !primary_audience.contains(audience) {
                        primary_audience.push(audience.clone());
                    };
                };
            };
            // TODO: remove
            if post.visibility == Visibility::Conversation &&
                in_reply_to.visibility == Visibility::Followers
            {
                // Add followers of a parent post author to audience
                let maybe_in_reply_to_followers = match in_reply_to.author.actor_json {
                    Some(ref actor_data) => {
                        actor_data.followers.as_ref().map(|followers| {
                            compatible_id(actor_data, followers)
                                .expect("actor ID should be valid")
                        })
                    },
                    None => {
                        // Can't use "authority" parameter here
                        // because parent post author may have a different one
                        let actor_id = local_actor_id(
                            instance_url,
                            &in_reply_to.author.username,
                        );
                        let followers = LocalActorCollection::Followers.of(&actor_id);
                        Some(followers)
                    },
                };
                if let Some(in_reply_to_followers) = maybe_in_reply_to_followers {
                    if !primary_audience.contains(&in_reply_to_followers) {
                        primary_audience.push(in_reply_to_followers);
                    };
                };
            };
            Some(compatible_post_object_id(instance_url, in_reply_to))
        },
        None => None,
    };
    let maybe_context_collection_id = if post.in_reply_to_id.is_none() {
        let conversation = post.expect_conversation();
        // TODO: FEP-EF61: use Authority
        let context_collection_id =
            local_conversation_collection(instance_url, conversation.id);
        Some(context_collection_id)
    } else {
        None
    };
    let replies_collection_id = local_object_replies(&object_id);

    Note {
        _context: with_context.then(build_default_context),
        id: object_id,
        object_type: object_type.to_string(),
        attachment: attachments,
        attributed_to: actor_id,
        in_reply_to: in_reply_to_object_id,
        context: maybe_context_collection_id,
        replies: replies_collection_id,
        content: post.content.clone(),
        content_map: post.language
            .and_then(|language| language.to_639_1())
            .map(|code| {
                HashMap::from([(code.to_owned(), post.content.clone())])
            }),
        sensitive: post.is_sensitive,
        tag: tags,
        one_of: one_of,
        any_of: any_of,
        end_time: end_time,
        to: primary_audience,
        cc: secondary_audience,
        quote_url: maybe_quote_url,
        published: post.created_at,
        updated: post.updated_at,
    }
}

pub async fn get_note_recipients(
    db_client: &impl DatabaseClient,
    post: &Post,
) -> Result<Vec<Recipient>, DatabaseError> {
    let mut primary_audience = vec![];
    let mut secondary_audience = vec![];
    match post.visibility {
        Visibility::Public | Visibility::Followers => {
            let followers = get_followers(db_client, post.author.id).await?;
            secondary_audience.extend(followers);
        },
        Visibility::Subscribers => {
            let subscribers = get_subscribers(db_client, post.author.id).await?;
            secondary_audience.extend(subscribers);
        },
        Visibility::Conversation => {
            let conversation = post.expect_conversation();
            let owner = get_post_author(db_client, conversation.root_id).await?;
            primary_audience.push(owner);
        },
        Visibility::Direct => (),
    };
    if let Some(in_reply_to_id) = post.in_reply_to_id {
        // TODO: use post.in_reply_to ?
        let in_reply_to_author = get_post_author(db_client, in_reply_to_id).await?;
        primary_audience.push(in_reply_to_author);
    };
    primary_audience.extend(post.mentions.clone());
    if let Some(ref poll) = post.poll {
        let voters = get_voters(db_client, poll.id).await?;
        secondary_audience.extend(voters);
    };

    let mut recipients = vec![];
    for profile in primary_audience {
        if let Some(remote_actor) = profile.actor_json {
            for mut recipient in Recipient::from_actor_data(&remote_actor) {
                recipient.is_primary = true;
                recipients.push(recipient);
            };
        };
    };
    for profile in secondary_audience {
        if let Some(remote_actor) = profile.actor_json {
            recipients.extend(Recipient::from_actor_data(&remote_actor));
        };
    };
    Ok(recipients)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::uuid;
    use mitra_models::{
        conversations::types::Conversation,
        polls::types::{Poll, PollResult, PollResults},
        posts::types::RelatedPosts,
        profiles::types::{DbActor, DbActorProfile},
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
        let author = DbActorProfile::local_for_test("author");
        let post = Post {
            author,
            tags: vec!["test".to_string()],
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
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
    fn test_build_question() {
        let author = DbActorProfile::local_for_test("author");
        let poll = Poll {
            id: uuid!("11fa64ff-b5a3-47bf-b23d-22b360581c3f"),
            multiple_choices: false,
            ends_at: DateTime::parse_from_rfc3339("2023-03-27T12:13:46Z")
                .unwrap().with_timezone(&Utc),
            results: PollResults::new(vec![
                PollResult::new("option 1"),
                PollResult::new("option 2"),
            ]),
        };
        let conversation = Conversation {
            id: uuid!("837ffc24-dab2-414b-a9b8-fe47d0a463f2"),
            ..Conversation::for_test(uuid!("11fa64ff-b5a3-47bf-b23d-22b360581c3f"))
        };
        let post = Post {
            id: conversation.root_id,
            author,
            conversation: Some(conversation),
            poll: Some(poll),
            created_at: DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
                .unwrap().with_timezone(&Utc),
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let question = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
            &post,
            true,
            true,
        );

        let value = serde_json::to_value(question).unwrap();
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v1",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji",
                    "litepub": "http://litepub.social/ns#",
                    "EmojiReact": "litepub:EmojiReact"
                },
            ],
            "id": "https://server.example/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f",
            "type": "Question",
            "attributedTo": "https://server.example/users/author",
            "content": "",
            "sensitive": false,
            "context": "https://server.example/collections/conversations/837ffc24-dab2-414b-a9b8-fe47d0a463f2",
            "replies": "https://server.example/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f/replies",
            "oneOf": [
                {
                    "type": "Note",
                    "name": "option 1",
                    "replies": {"totalItems": 0},
                },
                {
                    "type": "Note",
                    "name": "option 2",
                    "replies": {"totalItems": 0},
                },
            ],
            "endTime": "2023-03-27T12:13:46Z",
            "published": "2023-02-24T23:36:38Z",
            "to": [AP_PUBLIC],
            "cc": ["https://server.example/users/author/followers"],
        });
        assert_eq!(value, expected_value);
    }

    #[test]
    fn test_build_note_followers_only() {
        let post = Post {
            visibility: Visibility::Followers,
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
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
        let subscriber = DbActorProfile::remote_for_test(
            "subscriber",
            subscriber_id,
        );
        let post = Post {
            visibility: Visibility::Subscribers,
            mentions: vec![subscriber],
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
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
        let mentioned = DbActorProfile::remote_for_test(
            "mention",
            mentioned_id,
        );
        let post = Post {
            visibility: Visibility::Direct,
            mentions: vec![mentioned],
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
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
            related_posts: Some(RelatedPosts {
                in_reply_to: Some(Box::new(parent.clone())),
                ..Default::default()
            }),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
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
        let parent_author = DbActorProfile::remote_for_test_with_data(
            "test",
            DbActor {
                id: parent_author_actor_id.to_string(),
                url: Some(parent_author_actor_url.to_string()),
                ..Default::default()
            },
        );
        let parent = Post {
            author: parent_author.clone(),
            object_id: Some("https://test.net/obj/123".to_string()),
            ..Default::default()
        };
        let post = Post {
            in_reply_to_id: Some(parent.id),
            mentions: vec![parent_author],
            related_posts: Some(RelatedPosts {
                in_reply_to: Some(Box::new(parent.clone())),
                ..Default::default()
            }),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
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
    fn test_build_note_with_remote_parent_and_with_conversation() {
        let parent_author_actor_id = "https://social.example/user/test";
        let parent_author_followers = "https://social.example/user/test/followers";
        let parent_author_actor_url = "https://social.example/@test";
        let parent_author = DbActorProfile::remote_for_test_with_data(
            "test",
            DbActor {
                id: parent_author_actor_id.to_string(),
                followers: Some(parent_author_followers.to_string()),
                url: Some(parent_author_actor_url.to_string()),
                ..Default::default()
            },
        );
        let conversation = Conversation {
            id: uuid!("837ffc24-dab2-414b-a9b8-fe47d0a463f2"),
            ..Conversation::for_test(Default::default())
        };
        let parent = Post {
            id: conversation.root_id,
            conversation: Some(conversation),
            visibility: Visibility::Followers,
            ..Post::remote_for_test(
                &parent_author,
                "https://social.example/obj/123",
            )
        };
        let post = Post {
            id: uuid!("11fa64ff-b5a3-47bf-b23d-22b360581c3f"),
            conversation: parent.conversation.clone(),
            in_reply_to_id: Some(parent.id),
            visibility: Visibility::Conversation,
            mentions: vec![parent_author],
            created_at: DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
                .unwrap().with_timezone(&Utc),
            related_posts: Some(RelatedPosts {
                in_reply_to: Some(Box::new(parent.clone())),
                ..Default::default()
            }),
            ..Default::default()
        };
        let authority = Authority::server(INSTANCE_URL);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
            &post,
            false,
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
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji",
                    "litepub": "http://litepub.social/ns#",
                    "EmojiReact": "litepub:EmojiReact"
                },
            ],
            "id": "https://server.example/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f",
            "type": "Note",
            "attributedTo": "https://server.example/users/test",
            "inReplyTo": "https://social.example/obj/123",
            "content": "",
            "sensitive": false,
            "tag": [
                {
                    "type": "Mention",
                    "name": "@test@social.example",
                    "href": "https://social.example/user/test",
                },
            ],
            "replies": "https://server.example/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f/replies",
            "published": "2023-02-24T23:36:38Z",
            "to": [
                "https://social.example/user/test",
                "https://social.example/user/test/followers",
            ],
            "cc": [],
        });
        assert_eq!(value, expected_value);
    }

    #[test]
    fn test_build_note_fep_ef61() {
        let author = User::default();
        let conversation = Conversation {
            id: uuid!("837ffc24-dab2-414b-a9b8-fe47d0a463f2"),
            ..Conversation::for_test(uuid!("11fa64ff-b5a3-47bf-b23d-22b360581c3f"))
        };
        let post = Post {
            id: conversation.root_id,
            author: author.profile.clone(),
            conversation: Some(conversation),
            created_at: DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
                .unwrap().with_timezone(&Utc),
            related_posts: Some(RelatedPosts::default()),
            ..Default::default()
        };
        let authority = Authority::from_user(INSTANCE_URL, &author, true);
        let media_server = MediaServer::for_test(INSTANCE_URL);
        let note = build_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &authority,
            &media_server,
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
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji",
                    "litepub": "http://litepub.social/ns#",
                    "EmojiReact": "litepub:EmojiReact"
                },
            ],
            "id": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f",
            "type": "Note",
            "attributedTo": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "content": "",
            "sensitive": false,
            "context": "https://server.example/collections/conversations/837ffc24-dab2-414b-a9b8-fe47d0a463f2",
            "replies": "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/11fa64ff-b5a3-47bf-b23d-22b360581c3f/replies",
            "published": "2023-02-24T23:36:38Z",
            "to": [
                "https://www.w3.org/ns/activitystreams#Public",
            ],
            "cc": [
                "https://server.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/followers",
            ],
        });
        assert_eq!(value, expected_value);
    }
}
