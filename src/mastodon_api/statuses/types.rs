use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_activitypub::identifiers::{
    compatible_post_object_id,
    local_tag_collection,
    post_object_id,
    profile_actor_url,
};
use mitra_models::{
    emojis::types::DbEmoji,
    posts::types::{Post, Visibility},
    profiles::types::DbActorProfile,
};

use crate::mastodon_api::{
    accounts::types::Account,
    custom_emojis::types::CustomEmoji,
    media::types::Attachment,
    media_server::ClientMediaServer,
    polls::types::Poll,
};

pub const POST_CONTENT_TYPE_HTML: &str = "text/html";
pub const POST_CONTENT_TYPE_MARKDOWN: &str = "text/markdown";

/// https://docs.joinmastodon.org/entities/mention/
#[derive(Serialize)]
pub struct Mention {
    id: String,
    username: String,
    acct: String,
    url: String,
}

impl Mention {
    fn from_profile(instance_url: &str, profile: DbActorProfile) -> Self {
        Mention {
            id: profile.id.to_string(),
            username: profile.username.clone(),
            acct: profile.preferred_handle().to_owned(),
            url: profile_actor_url(instance_url, &profile),
        }
    }
}

/// https://docs.joinmastodon.org/entities/tag/
#[derive(Serialize)]
pub struct Tag {
    name: String,
    url: String,
}

impl Tag {
    pub fn from_tag_name(instance_url: &str, tag_name: String) -> Self {
        let tag_url = local_tag_collection(instance_url, &tag_name);
        Tag {
            name: tag_name,
            url: tag_url,
        }
    }
}

#[derive(Serialize)]
struct PleromaEmojiReaction {
    account_ids: Vec<Uuid>,
    count: i32,
    me: bool,
    name: String,
    url: Option<String>,
}

/// https://docs-develop.pleroma.social/backend/development/API/differences_in_mastoapi_responses/#statuses
#[derive(Serialize)]
struct PleromaData {
    emoji_reactions: Vec<PleromaEmojiReaction>,
    in_reply_to_account_acct: Option<String>,
    parent_visible: bool,
    quote: Option<Box<Status>>,
    quote_visible: bool,
}

/// https://docs.joinmastodon.org/entities/status/
#[derive(Serialize)]
pub struct Status {
    pub id: Uuid,
    pub uri: String,
    url: String,
    pub created_at: DateTime<Utc>,
    edited_at: Option<DateTime<Utc>>,
    pub account: Account,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    in_reply_to_account_id: Option<Uuid>,
    pub reblog: Option<Box<Status>>,
    pub visibility: String,
    pub sensitive: bool,
    pub spoiler_text: String,
    pub pinned: bool,
    pub replies_count: i32,
    pub favourites_count: i32,
    pub reblogs_count: i32,
    poll: Option<Poll>,
    pub media_attachments: Vec<Attachment>,
    mentions: Vec<Mention>,
    tags: Vec<Tag>,
    emojis: Vec<CustomEmoji>,

    // Authorized user attributes
    pub favourited: bool,
    pub reblogged: bool,
    bookmarked: bool,

    // Pleroma API
    pleroma: PleromaData,

    // Extra fields
    pub ipfs_cid: Option<String>,
    links: Vec<Status>,
}

impl Status {
    pub fn from_post(
        instance_url: &str,
        media_server: &ClientMediaServer,
        post: Post,
    ) -> Self {
        let object_id = post_object_id(instance_url, &post);
        let object_url = compatible_post_object_id(instance_url, &post);
        let maybe_poll = if let Some(ref db_poll) = post.poll {
            let maybe_voted_for = post.actions.as_ref()
                .map(|actions| actions.voted_for.clone());
            let poll = Poll::from_db(db_poll, maybe_voted_for);
            Some(poll)
        } else {
            None
        };
        let attachments: Vec<Attachment> = post.attachments.into_iter()
            .map(|item| Attachment::from_db(media_server, item))
            .collect();
        let mentions: Vec<Mention> = post.mentions.into_iter()
            .map(|item| Mention::from_profile(instance_url, item))
            .collect();
        let tags: Vec<Tag> = post.tags.into_iter()
            .map(|tag_name| Tag::from_tag_name(instance_url, tag_name))
            .collect();
        let emojis: Vec<CustomEmoji> = post.emojis.into_iter()
            .map(|emoji| CustomEmoji::from_db(media_server, emoji))
            .collect();
        let account = Account::from_profile(
            instance_url,
            media_server,
            post.author,
        );
        let reblog = if let Some(repost_of) = post.repost_of {
            let status = Status::from_post(instance_url, media_server, *repost_of);
            Some(Box::new(status))
        } else {
            None
        };
        let maybe_quote = post.linked.first().cloned().map(|post| {
            let status = Status::from_post(instance_url, media_server, post);
            Box::new(status)
        });
        let links: Vec<Status> = post.linked.into_iter().map(|post| {
            Status::from_post(instance_url, media_server, post)
        }).collect();
        let visibility = match post.visibility {
            Visibility::Public => "public",
            Visibility::Direct => "direct",
            Visibility::Followers => "private",
            Visibility::Subscribers => "subscribers",
            Visibility::Conversation => "conversation",
        };
        let mut emoji_reactions = vec![];
        let mut favourites_count = 0;
        for reaction in post.reactions {
            let content = if let Some(content) = reaction.content {
                content
            } else {
                favourites_count += reaction.count;
                continue;
            };
            let maybe_custom_emoji = reaction.emoji
                .map(|emoji| CustomEmoji::from_db(media_server, emoji));
            let reaction = PleromaEmojiReaction {
                account_ids: reaction.authors,
                count: reaction.count,
                me: post.actions.as_ref().map_or(false, |actions| {
                    actions.reacted_with.contains(&content)
                }),
                // Emoji name or emoji symbol
                name: maybe_custom_emoji.as_ref()
                    .map(|emoji| emoji.shortcode.clone())
                    .unwrap_or(content),
                url: maybe_custom_emoji.map(|emoji| emoji.url),
            };
            emoji_reactions.push(reaction);
        };
        Self {
            id: post.id,
            uri: object_id,
            url: object_url,
            created_at: post.created_at,
            edited_at: post.updated_at,
            account: account,
            content: post.content,
            in_reply_to_id: post.in_reply_to_id,
            in_reply_to_account_id: post.in_reply_to.as_ref()
                .map(|post| post.author.id),
            reblog: reblog,
            visibility: visibility.to_string(),
            sensitive: post.is_sensitive,
            spoiler_text: "".to_string(),
            pinned: post.is_pinned,
            replies_count: post.reply_count,
            favourites_count: favourites_count,
            reblogs_count: post.repost_count,
            poll: maybe_poll,
            media_attachments: attachments,
            mentions: mentions,
            tags: tags,
            emojis: emojis,
            favourited: post.actions.as_ref().map_or(false, |actions| actions.liked),
            reblogged: post.actions.as_ref().map_or(false, |actions| actions.reposted),
            bookmarked: post.actions.as_ref().map_or(false, |actions| actions.bookmarked),
            pleroma: PleromaData {
                emoji_reactions,
                in_reply_to_account_acct: post.in_reply_to
                    .map(|post| post.author.preferred_handle().to_owned()),
                parent_visible: post.parent_visible,
                quote_visible: maybe_quote.is_some(),
                quote: maybe_quote,
            },
            ipfs_cid: post.ipfs_cid,
            links: links,
        }
    }
}

#[derive(Serialize)]
pub struct StatusTombstone {
    #[serde(flatten)]
    pub status: Status,
    pub text: String,
}

fn default_post_content_type() -> String { POST_CONTENT_TYPE_MARKDOWN.to_string() }

/// https://docs.joinmastodon.org/methods/statuses/
#[derive(Debug, Deserialize)]
pub struct StatusData {
    pub status: String,

    #[serde(default, alias = "media_ids[]")]
    pub media_ids: Vec<Uuid>,

    pub in_reply_to_id: Option<Uuid>,
    pub visibility: Option<String>,

    #[serde(default)]
    pub sensitive: bool,

    #[serde(default = "default_post_content_type")]
    pub content_type: String,

    // Pleroma API
    pub quote_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct StatusPreviewData {
    pub status: String,

    #[serde(default = "default_post_content_type")]
    pub content_type: String,
}

#[derive(Serialize)]
pub struct StatusPreview {
    pub content: String,
    pub emojis: Vec<CustomEmoji>
}

impl StatusPreview {
    pub fn new(
        media_server: &ClientMediaServer,
        content: String,
        emojis: Vec<DbEmoji>,
    ) -> Self {
        let emojis: Vec<CustomEmoji> = emojis.into_iter()
            .map(|emoji| CustomEmoji::from_db(media_server, emoji))
            .collect();
        Self { content, emojis }
    }
}

/// https://docs.joinmastodon.org/entities/StatusSource/
#[derive(Serialize)]
pub struct StatusSource {
    id: Uuid,
    content_type: String, // Pleroma addon
    text: String,
    spoiler_text: String,
}

impl StatusSource {
    pub fn from_post(post: Post) -> Self {
        let (content_source, content_type) = match post.content_source {
            Some(source) => (source, POST_CONTENT_TYPE_MARKDOWN),
            None => (post.content, POST_CONTENT_TYPE_HTML),
        };
        Self {
            id: post.id,
            content_type: content_type.to_string(),
            text: content_source,
            spoiler_text: "".to_string(),
        }
    }
}

/// https://docs.joinmastodon.org/methods/statuses/#edit
#[derive(Deserialize)]
pub struct StatusUpdateData {
    pub status: String,

    #[serde(default, alias = "media_ids[]")]
    pub media_ids: Vec<Uuid>,

    #[serde(default)]
    pub sensitive: bool,

    #[serde(default = "default_post_content_type")]
    pub content_type: String,

    // Pleroma API
    pub quote_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct Context {
    pub ancestors: Vec<Status>,
    pub descendants: Vec<Status>,
}
