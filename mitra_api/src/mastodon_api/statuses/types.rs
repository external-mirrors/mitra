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
use mitra_utils::languages::Language;
use mitra_validators::errors::ValidationError;

use crate::mastodon_api::{
    accounts::types::Account,
    custom_emojis::types::CustomEmoji,
    media::types::Attachment,
    media_server::ClientMediaServer,
    pagination::PageSize,
    polls::types::Poll,
    reactions::types::PleromaEmojiReaction,
    serializers::{
        deserialize_language_code_opt,
        serialize_datetime,
        serialize_datetime_opt,
    },
};

pub const POST_CONTENT_TYPE_HTML: &str = "text/html";
pub const POST_CONTENT_TYPE_MARKDOWN: &str = "text/markdown";

/// https://docs.joinmastodon.org/entities/Quote/
#[derive(Serialize)]
struct Quote {
    state: &'static str,
    quoted_status: Box<Status>,
}

/// https://docs.joinmastodon.org/entities/mention/
#[derive(Serialize)]
pub struct Mention {
    id: String,
    username: String,
    acct: String,
    url: String,
}

impl Mention {
    fn from_profile(instance_uri: &str, profile: DbActorProfile) -> Self {
        Mention {
            id: profile.id.to_string(),
            username: profile.username.clone(),
            acct: profile.preferred_handle().to_owned(),
            url: profile_actor_url(instance_uri, &profile),
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
    pub fn from_tag_name(instance_uri: &str, tag_name: String) -> Self {
        let tag_url = local_tag_collection(instance_uri, &tag_name);
        Tag {
            name: tag_name,
            url: tag_url,
        }
    }
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
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_datetime_opt")]
    edited_at: Option<DateTime<Utc>>,
    pub account: Account,
    pub content: String,
    language: Option<String>,
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
    quote: Option<Quote>,
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
    hidden: bool,
    pub ipfs_cid: Option<String>,
    links: Vec<Status>,
}

impl Status {
    pub fn from_post(
        instance_uri: &str,
        media_server: &ClientMediaServer,
        post: Post,
    ) -> Self {
        let object_id = post_object_id(instance_uri, &post);
        let object_url = if let Some(url) = post.url {
            url
        } else {
            compatible_post_object_id(instance_uri, &post)
        };
        let maybe_poll = if let Some(ref db_poll) = post.poll {
            let maybe_voted_for = post.actions.as_ref()
                .map(|actions| actions.voted_for.clone());
            let poll = Poll::from_db(
                media_server,
                db_poll,
                post.emojis.clone(),
                maybe_voted_for,
            );
            Some(poll)
        } else {
            None
        };
        let attachments: Vec<Attachment> = post.attachments.into_iter()
            .map(|item| Attachment::from_db(media_server, item))
            .collect();
        let mentions: Vec<Mention> = post.mentions.into_iter()
            .map(|item| Mention::from_profile(instance_uri, item))
            .collect();
        let tags: Vec<Tag> = post.tags.into_iter()
            .map(|tag_name| Tag::from_tag_name(instance_uri, tag_name))
            .collect();
        let emojis: Vec<CustomEmoji> = post.emojis.into_iter()
            .map(|emoji| CustomEmoji::from_db(media_server, emoji))
            .collect();
        let account = Account::from_profile(
            instance_uri,
            media_server,
            post.author,
        );
        // Nested Status entities may be built without related_posts
        let related_posts = post.related_posts.unwrap_or_default();
        let reblog = if let Some(repost_of) = related_posts.repost_of {
            let status = Status::from_post(instance_uri, media_server, *repost_of);
            Some(Box::new(status))
        } else {
            None
        };
        let maybe_first_link = related_posts.linked.first();
        let maybe_quoted_status = maybe_first_link.cloned().map(|post| {
            let status = Status::from_post(instance_uri, media_server, post);
            Box::new(status)
        });
        let maybe_quote = maybe_first_link.cloned().map(|post| {
            let status = Status::from_post(instance_uri, media_server, post);
            Quote { state: "accepted", quoted_status: Box::new(status) }
        });
        let links: Vec<Status> = related_posts.linked.into_iter().map(|post| {
            Status::from_post(instance_uri, media_server, post)
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
            let reacted = post.actions.as_ref()
                .is_some_and(|actions| actions.reacted_with.contains(&content));
            let reaction = PleromaEmojiReaction {
                // Emoji name or emoji symbol
                name: maybe_custom_emoji.as_ref()
                    .map(|emoji| emoji.shortcode.clone())
                    .unwrap_or(content),
                url: maybe_custom_emoji.map(|emoji| emoji.url),
                count: reaction.count,
                accounts: vec![],
                me: reacted,
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
            language: post.language
                .and_then(|language| language.to_639_1())
                .map(|code| code.to_owned()),
            in_reply_to_id: post.in_reply_to_id,
            in_reply_to_account_id: related_posts
                .in_reply_to.as_ref()
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
            quote: maybe_quote,
            mentions: mentions,
            tags: tags,
            emojis: emojis,
            favourited: post.actions.as_ref().is_some_and(|actions| actions.liked),
            reblogged: post.actions.as_ref().is_some_and(|actions| actions.reposted),
            bookmarked: post.actions.as_ref().is_some_and(|actions| actions.bookmarked),
            pleroma: PleromaData {
                emoji_reactions,
                in_reply_to_account_acct: related_posts
                    .in_reply_to
                    .map(|post| post.author.preferred_handle().to_owned()),
                parent_visible: post.parent_visible,
                quote_visible: maybe_quoted_status.as_ref()
                    .map(|status| !status.hidden)
                    .unwrap_or(true),
                quote: maybe_quoted_status,
            },
            hidden: post.actions.is_some_and(|actions| actions.hidden),
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

#[derive(Clone, Debug, Deserialize)]
pub struct PollParams {
    pub options: Vec<String>,
    pub expires_in: u32,
    pub multiple: Option<bool>,
}

pub fn visibility_from_str(value: &str) -> Result<Visibility, ValidationError> {
    let visibility = match value {
        "public" | "unlisted" => Visibility::Public,
        "direct" => Visibility::Direct,
        "private" => Visibility::Followers,
        "subscribers" => Visibility::Subscribers,
        "conversation" => Visibility::Conversation,
        _ => return Err(ValidationError("invalid visibility parameter")),
    };
    Ok(visibility)
}

// https://docs.joinmastodon.org/methods/statuses/
#[derive(Debug, Deserialize)]
pub struct StatusData {
    pub status: Option<String>,

    #[serde(default, deserialize_with = "deserialize_language_code_opt")]
    pub language: Option<Language>,

    #[serde(default, alias = "media_ids[]")]
    pub media_ids: Vec<Uuid>,

    pub in_reply_to_id: Option<Uuid>,
    pub visibility: Option<String>,

    #[serde(default)]
    pub sensitive: bool,

    // Poll parameters: JSON
    // https://docs.joinmastodon.org/client/intro/#hash
    pub poll: Option<PollParams>,

    // Poll parameters: form data
    #[serde(default, rename = "poll[options][]")]
    pub poll_options: Vec<String>,

    #[serde(rename = "poll[expires_in]")]
    pub poll_expires_in: Option<u32>,

    #[serde(rename = "poll[multiple]")]
    pub poll_multiple: Option<bool>,

    // Pleroma API
    #[serde(default = "default_post_content_type")]
    pub content_type: String,

    pub quote_id: Option<Uuid>,
}

impl StatusData {
    pub fn poll_params(&self) -> Result<Option<PollParams>, ValidationError> {
        let maybe_poll_params = if let Some(ref poll_params) = self.poll {
            Some(poll_params.clone())
        } else if !self.poll_options.is_empty() {
            let expires_in = self.poll_expires_in
                .ok_or(ValidationError("poll duration must be provided"))?;
            let poll_params = PollParams {
                options: self.poll_options.clone(),
                expires_in,
                multiple: self.poll_multiple,
            };
            Some(poll_params)
        } else {
            None
        };
        Ok(maybe_poll_params)
    }
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

    #[serde(default, deserialize_with = "deserialize_language_code_opt")]
    pub language: Option<Language>,

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

fn default_favourite_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct FavouritedByQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_favourite_list_page_size")]
    pub limit: PageSize,
}

#[derive(Deserialize)]
pub struct ReblogParams {
    pub visibility: Option<String>,
}

fn default_repost_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct RebloggedByQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_repost_list_page_size")]
    pub limit: PageSize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_from_post() {
        let instance_uri = "https://social.example";
        let media_server = ClientMediaServer::for_test("/media");
        let author = DbActorProfile::local_for_test("test");
        let post = Post {
            created_at: DateTime::parse_from_rfc3339("2023-02-24T23:36:38Z")
                .unwrap()
                .with_timezone(&Utc),
            ..Post::local_for_test(&author)
        };
        let status = Status::from_post(instance_uri, &media_server, post);
        assert_eq!(status.content, "");
        let status_json = serde_json::to_value(status).unwrap();
        assert_eq!(
            status_json["created_at"].as_str().unwrap(),
            "2023-02-24T23:36:38.000Z",
        );
    }
}
