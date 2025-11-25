use std::collections::HashMap;

use apx_sdk::{
    addresses::WebfingerAddress,
    constants::{AP_MEDIA_TYPE, AP_PUBLIC, AS_MEDIA_TYPE},
    core::url::canonical::{is_same_origin, CanonicalUri},
    deserialization::{
        deserialize_into_id_array,
        deserialize_into_link_href,
        deserialize_into_object_id_opt,
        deserialize_object_array,
        parse_into_href_array,
    },
    fetch::fetch_media,
    utils::is_public,
};
use chrono::{DateTime, Utc};
use serde::{
    Deserialize,
    Deserializer,
    de::{Error as DeserializerError},
};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_adapters::{
    permissions::filter_mentions,
    posts::check_post_limits,
};
use mitra_models::{
    activitypub::queries::save_attributed_object,
    attachments::queries::create_attachment,
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    filter_rules::types::FilterAction,
    media::types::MediaInfo,
    polls::types::{PollData, PollResult},
    posts::{
        helpers::can_link_post,
        queries::{create_post, update_post},
        types::{
            PostContext,
            PostCreateData,
            PostDetailed,
            PostUpdateData,
            Visibility,
        },
    },
    profiles::types::{
        DbActorProfile,
        Origin::Remote,
    },
};
use mitra_utils::{
    languages::{parse_language_tag, Language},
};
use mitra_validators::{
    errors::ValidationError,
    media::{validate_media_description, validate_media_url},
    polls::{clean_poll_option_name, validate_poll_data},
    posts::{
        clean_remote_content,
        clean_title,
        validate_content,
        validate_post_create_data,
        validate_post_mentions,
        validate_post_update_data,
        validate_reply,
        EMOJI_LIMIT,
        HASHTAG_LIMIT,
        LINK_LIMIT,
        MENTION_LIMIT,
    },
    tags::validate_hashtag,
};

use crate::{
    builders::note::LinkTag,
    filter::get_moderation_domain,
    identifiers::{
        canonicalize_id,
    },
    importers::{
        get_or_import_profile_by_webfinger_address,
        get_post_by_object_id,
        get_profile_by_actor_id,
        is_actor_importer_error,
        ActorIdResolver,
        ApClient,
    },
    ownership::parse_attributed_to,
    vocabulary::*,
};

use super::{
    emoji::handle_emoji,
    HandlerError,
};

fn deserialize_attributed_to<'de, D>(
    deserializer: D,
) -> Result<String, D::Error>
    where D: Deserializer<'de>
{
    let value = JsonValue::deserialize(deserializer)?;
    let attributed_to = parse_attributed_to(&value)
        .map_err(DeserializerError::custom)?;
    Ok(attributed_to)
}

fn deserialize_icon<'de, D>(
    deserializer: D,
) -> Result<Vec<MediaAttachment>, D::Error>
    where D: Deserializer<'de>
{
    let values: Vec<JsonValue> = deserialize_object_array(deserializer)?;
    let mut images = vec![];
    for value in values {
        match value["type"].as_str() {
            Some(IMAGE) => {
                match serde_json::from_value(value) {
                    Ok(image) => {
                        images.push(image);
                    },
                    Err(error) => {
                        log::warn!("invalid icon ({error})");
                    },
                };
            },
            _ => {
                log::warn!("unsupported icon type");
            },
        };
    };
    Ok(images)
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAttachment {
    #[serde(rename = "type")]
    attachment_type: String,

    name: Option<String>,
    summary: Option<String>,
    media_type: Option<String>,

    #[serde(deserialize_with = "deserialize_into_link_href")]
    pub url: String,
}

#[derive(Clone)]
pub enum Attachment {
    Media(MediaAttachment),
    Link(String),
}

fn deserialize_attachment<'de, D>(
    deserializer: D,
) -> Result<Vec<Attachment>, D::Error>
    where D: Deserializer<'de>
{
    let values: Vec<JsonValue> = deserialize_object_array(deserializer)?;
    let mut attachments = vec![];
    for value in values {
        match value["type"].as_str() {
            Some(AUDIO | DOCUMENT | IMAGE | VIDEO) => (),
            Some(LINK) => {
                // Lemmy compatibility
                let link_href = if let Some(href) = value["href"].as_str() {
                    href.to_string()
                } else {
                    log::warn!("invalid link attachment");
                    continue;
                };
                attachments.push(Attachment::Link(link_href));
                continue;
            },
            Some(attachment_type) => {
                log::warn!(
                    "skipping attachment of type {}",
                    attachment_type,
                );
                continue;
            },
            None => {
                log::warn!("attachment without type");
                continue;
            },
        };
        match serde_json::from_value(value) {
            Ok(attachment) => {
                attachments.push(Attachment::Media(attachment));
            },
            Err(error) => {
                log::warn!("invalid attachment ({error})");
                continue;
            },
        };
    };
    Ok(attachments)
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct AttributedObject {
    // https://www.w3.org/TR/activitypub/#obj-id
    // "id" and "type" are required properties
    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    // Required for conversion into "post" entity
    #[serde(deserialize_with = "deserialize_attributed_to")]
    attributed_to: String,

    name: Option<String>,
    pub content: Option<String>,
    content_map: Option<HashMap<String, String>>,
    media_type: Option<String>,
    pub sensitive: Option<bool>,
    summary: Option<String>,

    #[serde(
        default,
        deserialize_with = "deserialize_icon",
    )]
    icon: Vec<MediaAttachment>,

    #[serde(
        default,
        deserialize_with = "deserialize_attachment",
    )]
    pub attachment: Vec<Attachment>,

    #[serde(
        default,
        deserialize_with = "deserialize_object_array",
    )]
    tag: Vec<JsonValue>,

    #[serde(default, deserialize_with = "deserialize_into_object_id_opt")]
    pub in_reply_to: Option<String>,

    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    to: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    cc: Vec<String>,

    pub published: Option<DateTime<Utc>>,
    pub updated: Option<DateTime<Utc>>,
    url: Option<JsonValue>,

    // Polls
    one_of: Option<JsonValue>,
    any_of: Option<JsonValue>,
    end_time: Option<DateTime<Utc>>,
    closed: Option<DateTime<Utc>>,

    quote: Option<String>,
    quote_url: Option<String>,

    // TODO: Use is_object?
    inbox: Option<String>,
}

impl AttributedObject {
    pub fn check_not_actor(&self) -> Result<(), ValidationError> {
        if self.inbox.is_some() {
            return Err(ValidationError("object is actor"));
        };
        Ok(())
    }

    fn is_converted(&self) -> bool {
        ![NOTE, QUESTION, CHAT_MESSAGE].contains(&self.object_type.as_str())
    }

    pub fn audience(&self) -> Vec<&String> {
        self.to.iter().chain(self.cc.iter()).collect()
    }

    fn language(&self) -> Option<Language> {
        let language_tag = self.content_map
            .as_ref()
            .and_then(|content_map| {
                content_map.iter()
                    .find(|(_, content)| self.content.as_ref() == Some(content))
                    .map(|(language_tag, _)| language_tag)
                    .or_else(|| {
                        log::warn!("content is not found in contentMap");
                        None
                    })
            })?;
        parse_language_tag(language_tag).or_else(|| {
            log::warn!("invalid language tag: {language_tag}");
            None
        })
    }

    fn quote(&self) -> Option<&String> {
        self.quote.as_ref()
            // Ignore Bookwyrm quotes
            // https://github.com/bookwyrm-social/bookwyrm/issues/3731
            .filter(|_| !self.id.contains("/quotation/"))
            .or(self.quote_url.as_ref())
    }
}

pub struct AttributedObjectJson {
    pub inner: AttributedObject,
    pub value: JsonValue,
}

impl<'de> Deserialize<'de> for AttributedObjectJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let value = JsonValue::deserialize(deserializer)?;
        let inner = serde_json::from_value(value.clone())
            .map_err(DeserializerError::custom)?;
        Ok(Self { inner, value })
    }
}

impl AttributedObjectJson {
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn attributed_to(&self) -> &str {
        &self.inner.attributed_to
    }

    pub fn in_reply_to(&self) -> Option<&str> {
        self.inner.in_reply_to.as_deref()
    }

    pub fn links(&self) -> Vec<String> {
        get_object_links(&self.inner)
    }
}

fn get_object_url(
    object: &AttributedObject,
) -> Result<Option<String>, ValidationError> {
    let maybe_object_url = match &object.url {
        Some(value) => {
            let urls = parse_into_href_array(value)
                .map_err(|_| ValidationError("invalid object URL"))?;
            // TODO: select URL with text/html media type
            urls.into_iter().next()
        },
        None => None,
    };
    Ok(maybe_object_url)
}

/// Get post content by concatenating name/summary and content
pub(super) fn get_object_content(object: &AttributedObject) ->
    Result<String, ValidationError>
{
    let title = if object.in_reply_to.is_none() {
        // Only top level posts can have titles
        object.name.as_ref()
            // NOTE: Mastodon uses 'summary' for content warnings
            // NOTE: 'summary' may contain HTML
            .or(object.summary.as_ref())
            .map(|title| clean_title(title))
            .filter(|title| !title.is_empty())
            .map(|title| format!("<h1>{}</h1>", title))
            .unwrap_or("".to_string())
    } else {
        "".to_string()
    };
    let content = if let Some(ref content) = object.content {
        if object.media_type == Some("text/markdown".to_string()) {
            format!("<p>{}</p>", content)
        } else {
            // HTML
            content.clone()
        }
    } else {
        "".to_string()
    };
    let content = format!("{}{}", title, content);
    let content_safe = clean_remote_content(&content);
    validate_content(&content_safe)?;
    Ok(content_safe)
}

fn create_content_link(url: &str) -> String {
    format!(
        r#"<p><a href="{0}" rel="noopener">{0}</a></p>"#,
        url,
    )
}

fn is_gnu_social_link(author_id: &str, attachment: &MediaAttachment) -> bool {
    if !author_id.contains("/index.php/user/") {
        return false;
    };
    if attachment.attachment_type != DOCUMENT {
        return false;
    };
    match attachment.media_type.as_ref() {
        None => true,
        Some(media_type) if media_type.contains("text/html") => true,
        _ => false,
    }
}

async fn get_object_attachments(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    object: &AttributedObject,
    author: &DbActorProfile,
) -> Result<(Vec<Uuid>, Vec<String>), HandlerError> {
    let agent = ap_client.agent();
    let author_hostname = get_moderation_domain(author.expect_actor_data())?;
    let is_filter_enabled = ap_client.filter.is_action_required(
        author_hostname.as_str(),
        FilterAction::RejectMediaAttachments,
    );
    let is_proxy_enabled = ap_client.filter.is_action_required(
        author_hostname.as_str(),
        FilterAction::ProxyMedia,
    );

    let mut values = object.attachment.clone();
    if object.object_type == VIDEO {
        // PeerTube video thumbnails
        let thumbnails = object.icon.iter().cloned().map(Attachment::Media);
        values.extend(thumbnails.take(1));
    };

    let mut attachments = vec![];
    let mut unprocessed = vec![];
    let mut downloaded: Vec<(MediaInfo, Option<String>)> = vec![];
    for attachment_value in values {
        let attachment = match attachment_value {
            Attachment::Media(attachment) => attachment,
            Attachment::Link(link) => {
                unprocessed.push(link);
                continue;
            },
        };
        if is_gnu_social_link(
            author.expect_remote_actor_id(),
            &attachment,
        ) {
            // Don't fetch HTML pages attached by GNU Social
            continue;
        };
        let attachment_url = attachment.url;
        if let Err(error) = validate_media_url(&attachment_url) {
            log::warn!("invalid attachment URL ({error}): {attachment_url}");
            continue;
        };
        if downloaded.iter().any(|(media, ..)| media.url() == Some(&attachment_url)) {
            // Already downloaded
            log::warn!("skipping duplicate attachment: {attachment_url}");
            continue;
        };
        let maybe_description = attachment.name
            // Used by GoToSocial
            .or(attachment.summary)
            .filter(|name| {
                validate_media_description(name)
                    .map_err(|error| log::warn!("{error}"))
                    .is_ok()
            });
        if is_filter_enabled {
            // Do not download
            log::warn!("attachment removed by filter: {attachment_url}");
            unprocessed.push(attachment_url);
            continue;
        };
        if downloaded.len() >= ap_client.limits.posts.attachment_limit {
            // Stop downloading if limit is reached
            log::warn!("too many attachments");
            unprocessed.push(attachment_url);
            continue;
        };
        let (file_data, media_type) = match fetch_media(
            &agent,
            &attachment_url,
            &ap_client.limits.media.supported_media_types(),
            ap_client.limits.media.file_size_limit,
        ).await {
            Ok(file) => file,
            Err(error) => {
                log::warn!(
                    "failed to fetch attachment ({}): {}",
                    attachment_url,
                    error,
                );
                unprocessed.push(attachment_url);
                continue;
            },
        };
        let media_info = if is_proxy_enabled {
            log::info!("linked attachment {}", attachment_url);
            MediaInfo::link(media_type, attachment_url)
        } else {
            let file_info = ap_client.media_storage
                .save_file(file_data, &media_type)?;
            log::info!("downloaded attachment {}", attachment_url);
            MediaInfo::remote(file_info, attachment_url)
        };
        downloaded.push((media_info, maybe_description));
    };
    let db_client = &**get_database_client(db_pool).await?;
    for (media_info, description) in downloaded {
        let db_attachment = create_attachment(
            db_client,
            author.id,
            media_info,
            description.as_deref(),
        ).await?;
        attachments.push(db_attachment.id);
    };
    Ok((attachments, unprocessed))
}

#[derive(Deserialize)]
struct Tag {
    name: Option<String>,
    href: Option<String>,
}

fn normalize_hashtag(tag: &str) -> Result<String, ValidationError> {
    let tag_name = tag.trim_start_matches('#');
    validate_hashtag(tag_name)?;
    Ok(tag_name.to_lowercase())
}

fn get_object_links(
    object: &AttributedObject,
) -> Vec<String> {
    let mut links = vec![];
    for tag_value in object.tag.clone() {
        let tag_type = tag_value["type"].as_str().unwrap_or(HASHTAG);
        if tag_type == LINK {
            let tag: LinkTag = match serde_json::from_value(tag_value) {
                Ok(tag) => tag,
                Err(_) => {
                    log::warn!("invalid link tag");
                    continue;
                },
            };
            if tag.media_type != AP_MEDIA_TYPE &&
                tag.media_type != AS_MEDIA_TYPE
            {
                // Unknown media type
                continue;
            };
            if !links.contains(&tag.href) {
                links.push(tag.href);
            };
        };
    };
    if let Some(object_id) = object.quote() {
        if !links.contains(object_id) {
            links.push(object_id.to_owned());
        };
    };
    links
}

async fn get_object_tags(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    object: &AttributedObject,
    author: &DbActorProfile,
    redirects: &HashMap<String, String>,
) -> Result<(Vec<Uuid>, Vec<String>, Vec<Uuid>, Vec<Uuid>), HandlerError> {
    let instance = &ap_client.instance;
    let moderation_domain = get_moderation_domain(author.expect_actor_data())?;

    let mut hashtag_count = 0;
    let mut mention_count = 0;
    let mut link_count = 0;
    let mut emoji_count = 0;

    let mut hashtags = vec![];
    let mut mentions = vec![];
    let mut links = vec![];
    let mut emojis = vec![];

    for tag_value in object.tag.clone() {
        let tag_type = tag_value["type"].as_str().unwrap_or(HASHTAG);
        if tag_type == HASHTAG {
            hashtag_count += 1;
            if hashtag_count > HASHTAG_LIMIT {
                continue;
            };
            let tag: Tag = match serde_json::from_value(tag_value) {
                Ok(tag) => tag,
                Err(_) => {
                    log::warn!("invalid hashtag");
                    continue;
                },
            };
            if let Some(tag_name) = tag.name {
                // Ignore invalid tags
                if let Ok(tag_name) = normalize_hashtag(&tag_name) {
                    if !hashtags.contains(&tag_name) {
                        hashtags.push(tag_name);
                    };
                } else {
                    log::warn!("invalid hashtag: {}", tag_name);
                };
            };
        } else if tag_type == MENTION {
            mention_count += 1;
            if mention_count > MENTION_LIMIT {
                continue;
            };
            let tag: Tag = match serde_json::from_value(tag_value) {
                Ok(tag) => tag,
                Err(_) => {
                    log::warn!("invalid mention");
                    continue;
                },
            };
            // Try to find profile by actor ID.
            if let Some(href) = tag.href {
                // NOTE: `href` attribute is usually actor ID
                // but also can be actor URL (profile link).
                match ActorIdResolver::default().resolve(
                    ap_client,
                    db_pool,
                    &href,
                ).await {
                    Ok(profile) => {
                        if !mentions.contains(&profile.id) {
                            mentions.push(profile.id);
                        };
                        continue;
                    },
                    Err(error) if is_actor_importer_error(&error) => {
                        log::warn!(
                            "failed to find mentioned profile by ID {}: {}",
                            href,
                            error,
                        );
                    },
                    Err(other_error) => return Err(other_error),
                };
            };
            // Try to find profile by webfinger address
            let tag_name = match tag.name {
                Some(name) => name,
                None => {
                    log::warn!("failed to parse mention");
                    continue;
                },
            };
            if let Ok(webfinger_address) = WebfingerAddress::from_handle(&tag_name) {
                let profile = match get_or_import_profile_by_webfinger_address(
                    ap_client,
                    db_pool,
                    &webfinger_address,
                ).await {
                    Ok(profile) => profile,
                    Err(error) if is_actor_importer_error(&error) => {
                        // Ignore mention if fetcher fails
                        // Ignore mention if local address is not valid
                        log::warn!(
                            "failed to find mentioned profile {}: {}",
                            webfinger_address,
                            error,
                        );
                        continue;
                    },
                    Err(other_error) => return Err(other_error),
                };
                log::info!("found mentioned profile by 'name': {tag_name}");
                if !mentions.contains(&profile.id) {
                    mentions.push(profile.id);
                };
            } else {
                log::warn!("failed to parse mention {}", tag_name);
            };
        } else if tag_type == LINK {
            link_count += 1;
            if link_count > LINK_LIMIT {
                continue;
            };
            let tag: LinkTag = match serde_json::from_value(tag_value) {
                Ok(tag) => tag,
                Err(_) => {
                    log::warn!("invalid link tag");
                    continue;
                },
            };
            if tag.media_type != AP_MEDIA_TYPE &&
                tag.media_type != AS_MEDIA_TYPE
            {
                // Unknown media type
                continue;
            };
            let href = redirects.get(&tag.href).unwrap_or(&tag.href);
            let canonical_linked_id = canonicalize_id(href)?;
            let linked = get_post_by_object_id(
                db_client_await!(db_pool),
                instance.uri_str(),
                &canonical_linked_id,
            ).await?;
            if !can_link_post(&linked) {
                log::warn!("post can not be linked");
                continue;
            };
            if !links.contains(&linked.id) {
                links.push(linked.id);
            };
        } else if tag_type == EMOJI {
            emoji_count += 1;
            if emoji_count > EMOJI_LIMIT {
                continue;
            };
            match handle_emoji(
                ap_client,
                db_pool,
                &moderation_domain,
                tag_value,
            ).await? {
                Some(emoji) => {
                    if !emojis.contains(&emoji.id) {
                        emojis.push(emoji.id);
                    };
                },
                None => continue,
            };
        } else {
            log::warn!("skipping tag of type {}", tag_type);
        };
    };

    // Create mentions for known actors in "to" and "cc" fields
    let audience = get_audience(object)?;
    let db_client = &**get_database_client(db_pool).await?;
    for target_id in audience {
        if is_public(&target_id) {
            continue;
        };
        if mentions.len() >= MENTION_LIMIT {
            log::warn!("not adding targets to mention list");
            break;
        };
        match get_profile_by_actor_id(
            db_client,
            instance.uri_str(),
            &target_id,
        ).await {
            Ok(profile) => {
                if !mentions.contains(&profile.id) {
                    mentions.push(profile.id);
                };
            },
            // Ignore unknown targets
            Err(DatabaseError::NotFound(_)) => continue,
            Err(other_error) => return Err(other_error.into()),
        };
    };

    // Parse quoteUrl as an object link
    if let Some(quote_id) = object.quote() {
        let object_id = redirects.get(quote_id).unwrap_or(quote_id);
        let canonical_object_id = canonicalize_id(object_id)?;
        let linked = get_post_by_object_id(
            db_client,
            instance.uri_str(),
            &canonical_object_id,
        ).await?;
        if can_link_post(&linked) {
            if links.len() < LINK_LIMIT && !links.contains(&linked.id) {
                links.push(linked.id);
            };
        } else {
            log::warn!("post can not be linked");
        };
    };

    if hashtag_count > HASHTAG_LIMIT {
        log::warn!("too many hashtags: {hashtag_count}");
    };
    if mention_count > MENTION_LIMIT {
        log::warn!("too many mentions: {mention_count}");
    };
    if link_count > LINK_LIMIT {
        log::warn!("too many links: {link_count}");
    };
    if emoji_count > EMOJI_LIMIT {
        log::warn!("too many emojis: {emoji_count}");
    };
    Ok((mentions, hashtags, links, emojis))
}

pub fn normalize_audience(
    audience: &[impl AsRef<str>],
) -> Result<Vec<CanonicalUri>, ValidationError> {
    let mut normalized_audience = audience.iter()
        .map(|target_id| {
            let normalized_target_id = if is_public(target_id) {
                AP_PUBLIC
            } else {
                target_id.as_ref()
            };
            canonicalize_id(normalized_target_id)
                .map_err(|_| ValidationError("invalid target ID"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    normalized_audience.sort_by_key(|id| id.to_string());
    normalized_audience.dedup_by_key(|id| id.to_string());
    Ok(normalized_audience)
}

pub(super) fn get_audience(
    object: &AttributedObject,
) -> Result<Vec<String>, ValidationError> {
    let mut audience = vec![];
    for target_id in normalize_audience(&object.audience())? {
        audience.push(target_id.to_string());
    };
    Ok(audience)
}

fn get_object_visibility(
    author: &DbActorProfile,
    audience: &[String],
    maybe_in_reply_to: Option<&PostDetailed>,
) -> (Visibility, PostContext) {
    let actor = author.expect_actor_data();
    if let Some(in_reply_to) = maybe_in_reply_to {
        let conversation = in_reply_to.expect_conversation();
        let context = PostContext::Reply {
            conversation_id: conversation.id,
            in_reply_to_id: in_reply_to.id,
        };
        let visibility = if let Some(ref conversation_audience) = conversation.audience {
            if conversation_audience == AP_PUBLIC {
                if audience.contains(conversation_audience) {
                    Visibility::Public
                } else if audience.iter().any(|id| Some(id) == actor.followers.as_ref()) {
                    // Narrowing down the scope from Public to Followers
                    Visibility::Followers
                } else {
                    // DM or unknown audience
                    Visibility::Direct
                }
            } else if audience.contains(conversation_audience) {
                // TODO: check scope widening
                Visibility::Conversation
            } else {
                #[allow(clippy::collapsible_else_if)]
                if audience.iter().any(|id| id == AP_PUBLIC) {
                    log::warn!("changing visibility from Public to Conversation");
                    Visibility::Conversation
                } else if audience.iter().any(|id| Some(id) == actor.followers.as_ref()) {
                    log::warn!("changing visibility from Followers to Conversation");
                    Visibility::Conversation
                } else {
                    // DM or unknown audience
                    Visibility::Direct
                }
            }
        } else {
            // No audience: DM or a legacy limited conversation
            Visibility::Direct
        };
        (visibility, context)
    } else {
        let mut conversation_audience = None;
        let visibility = if audience.iter().any(is_public) {
            conversation_audience = Some(AP_PUBLIC.to_owned());
            Visibility::Public
        } else if audience.iter().any(|id| Some(id) == actor.followers.as_ref()) {
            conversation_audience = actor.followers.clone();
            Visibility::Followers
        } else if audience.iter().any(|id| Some(id) == actor.subscribers.as_ref()) {
            conversation_audience = actor.subscribers.clone();
            Visibility::Subscribers
        } else {
            Visibility::Direct
        };
        let context = PostContext::Top {
            audience: conversation_audience,
        };
        (visibility, context)
    }
}

fn parse_poll_results(
    object: &AttributedObject,
) -> Result<PollData, ValidationError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Replies {
        total_items: u32,
    }
    #[derive(Deserialize)]
    struct Note {
        name: String,
        replies: Replies,
    }

    let (values, is_multichoice) = match (
        object.one_of.as_ref(),
        object.any_of.as_ref(),
    ) {
        // Single choice
        (Some(values), None) => (values, false),
        // Multiple choices
        (None, Some(values)) => (values, true),
        _ => return Err(ValidationError("invalid poll")),
    };
    let values = values
        .as_array()
        .ok_or(ValidationError("invalid poll options"))?;

    let mut results = vec![];
    for note_value in values {
        let note: Note = serde_json::from_value(note_value.clone())
            .map_err(|_| ValidationError("invalid poll option"))?;
        let result = PollResult {
            option_name: clean_poll_option_name(&note.name),
            vote_count: note.replies.total_items,
        };
        results.push(result);
    };
    let ends_at = object.end_time
        // Pleroma uses closed property even when poll is still active
        .or(object.closed);
    let poll_data = PollData {
        multiple_choices: is_multichoice,
        ends_at: ends_at,
        results: results,
    };
    validate_poll_data(&poll_data)?;
    Ok(poll_data)
}

pub async fn create_remote_post(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    object: AttributedObjectJson,
    redirects: &HashMap<String, String>,
) -> Result<PostDetailed, HandlerError> {
    let AttributedObjectJson { inner: object, value: object_value } = object;
    let canonical_object_id = canonicalize_id(&object.id)?;

    object.check_not_actor()?;
    if object.is_converted() {
        // Attempting to convert any object that has attributedTo property
        // into post
        log::info!("processing object of type {}", object.object_type);
    };

    if !is_same_origin(&object.attributed_to, &object.id)
        .map_err(|_| ValidationError("invalid object ID"))?
    {
        return Err(ValidationError("object attributed to actor from different server").into());
    };
    let author = ActorIdResolver::default().only_remote().resolve(
        ap_client,
        db_pool,
        &object.attributed_to,
    ).await.map_err(|err| {
        log::warn!("failed to import {} ({})", object.attributed_to, err);
        err
    })?;
    let author_hostname = get_moderation_domain(author.expect_actor_data())?;

    let maybe_in_reply_to = match object.in_reply_to {
        Some(ref object_id) => {
            let object_id = redirects.get(object_id).unwrap_or(object_id);
            let canonical_object_id = canonicalize_id(object_id)?;
            let in_reply_to = get_post_by_object_id(
                db_client_await!(db_pool),
                ap_client.instance.uri_str(),
                &canonical_object_id,
            ).await?;
            Some(in_reply_to)
        },
        None => None,
    };

    let mut content = get_object_content(&object)?;
    let maybe_poll_data = if object.object_type == QUESTION {
        match parse_poll_results(&object) {
            Ok(poll_data) => Some(poll_data),
            Err(error) => {
                log::warn!("{error}: {}", object.id);
                None
            },
        }
    } else {
        None
    };
    let maybe_object_url = get_object_url(&object)?;
    if object.is_converted() {
        // Append link to object
        let url = maybe_object_url.as_ref().unwrap_or(&object.id);
        content += &create_content_link(url);
    };
    let (attachments, unprocessed) = get_object_attachments(
        ap_client,
        db_pool,
        &object,
        &author,
    ).await?;
    for attachment_url in unprocessed {
        content += &create_content_link(&attachment_url);
    };

    let (mentions, hashtags, links, emojis) = get_object_tags(
        ap_client,
        db_pool,
        &object,
        &author,
        redirects,
    ).await?;

    // TODO: use on local posts too
    let db_client = &mut **get_database_client(db_pool).await?;
    let mentions = filter_mentions(
        db_client,
        mentions,
        &author,
        maybe_in_reply_to.as_ref().map(|post| post.id),
    ).await?;

    let audience = get_audience(&object)?;
    let (visibility, context) = get_object_visibility(
        &author,
        &audience,
        maybe_in_reply_to.as_ref(),
    );
    let is_sensitive =
        object.sensitive.unwrap_or(false) ||
        ap_client.filter.is_action_required(
            author_hostname.as_str(),
            FilterAction::MarkSensitive,
        );
    let created_at = object.published.unwrap_or(Utc::now());

    if visibility == Visibility::Direct &&
        !mentions.iter().any(|profile| profile.is_local())
    {
        log::warn!("direct message has no local recipients");
    };

    let post_data = PostCreateData {
        id: None,
        context: context,
        content: content,
        content_source: None,
        language: object.language(),
        visibility,
        is_sensitive,
        poll: maybe_poll_data,
        attachments: attachments,
        mentions: mentions.iter().map(|profile| profile.id).collect(),
        tags: hashtags,
        links: links,
        emojis: emojis,
        url: maybe_object_url,
        object_id: Some(canonical_object_id.to_string()),
        created_at,
    };
    validate_post_create_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, post_data.visibility)?;
    if let Some(in_reply_to) = maybe_in_reply_to {
        // TODO: disallow scope widening (see also: get_related_posts)
        validate_reply(
            &in_reply_to,
            author.id,
            post_data.visibility,
            &post_data.mentions,
        ).unwrap_or_else(|error| log::warn!("{error}"));
    };
    check_post_limits(&ap_client.limits.posts, &post_data.attachments, Remote)?;
    let post = create_post(db_client, author.id, post_data).await?;
    save_attributed_object(
        db_client,
        &canonical_object_id.to_string(),
        &object_value,
        post.id,
    ).await?;
    Ok(post)
}

pub async fn update_remote_post(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    post: PostDetailed,
    object: &AttributedObjectJson,
) -> Result<PostDetailed, HandlerError> {
    assert!(!post.is_local());
    let AttributedObjectJson { inner: object, value: object_json } = object;
    let canonical_author_id = canonicalize_id(&object.attributed_to)?;
    if canonical_author_id.to_string() != post.author.expect_remote_actor_id() {
        return Err(ValidationError("object owner can't be changed").into());
    };
    let author_hostname = get_moderation_domain(post.author.expect_actor_data())?;

    let maybe_in_reply_to = match object.in_reply_to {
        Some(ref object_id) => {
            let canonical_object_id = canonicalize_id(object_id)?;
            let in_reply_to = get_post_by_object_id(
                db_client_await!(db_pool),
                ap_client.instance.uri_str(),
                &canonical_object_id,
            ).await?;
            Some(in_reply_to)
        },
        None => None,
    };
    if maybe_in_reply_to.as_ref().map(|in_reply_to| in_reply_to.id) != post.in_reply_to_id {
        return Err(ValidationError("inReplyTo can't be changed").into());
    };

    let mut content = get_object_content(object)?;
    let maybe_poll_data = if object.object_type == QUESTION {
        match parse_poll_results(object) {
            Ok(poll_data) => {
                if post.poll.is_some() {
                    Some(poll_data)
                } else {
                    log::warn!("poll can't be added to existing post");
                    None
                }
            },
            Err(error) => {
                log::warn!("{error}: {}", object.id);
                None
            },
        }
    } else {
        None
    };
    let maybe_object_url = get_object_url(object)?;
    if object.is_converted() {
        // Append link to object
        let url = maybe_object_url.as_ref().unwrap_or(&object.id);
        content += &create_content_link(url);
    };
    let (attachments, unprocessed) = get_object_attachments(
        ap_client,
        db_pool,
        object,
        &post.author,
    ).await?;
    for attachment_url in unprocessed {
        content += &create_content_link(&attachment_url);
    };
    let (mentions, hashtags, links, emojis) = get_object_tags(
        ap_client,
        db_pool,
        object,
        &post.author,
        &HashMap::new(),
    ).await?;
    let is_sensitive =
        object.sensitive.unwrap_or(false) ||
        ap_client.filter.is_action_required(
            author_hostname.as_str(),
            FilterAction::MarkSensitive,
        );

    let db_client = &mut **get_database_client(db_pool).await?;
    let mentions = filter_mentions(
        db_client,
        mentions,
        &post.author,
        post.in_reply_to_id,
    ).await?;
    if post.visibility == Visibility::Direct &&
        !mentions.iter().any(|profile| profile.is_local())
    {
        log::warn!("direct message has no local recipients");
    };

    let is_edited = post.is_edited(
        &content,
        maybe_poll_data.as_ref(),
        // TODO: attachments are always re-created
        &attachments,
    );
    let updated_at = if is_edited {
        Some(Utc::now())
    } else {
        post.updated_at
    };

    let post_data = PostUpdateData {
        content,
        content_source: None,
        language: object.language(),
        is_sensitive,
        poll: maybe_poll_data,
        attachments,
        mentions: mentions.iter().map(|profile| profile.id).collect(),
        tags: hashtags,
        links,
        emojis,
        url: maybe_object_url,
        updated_at,
    };
    validate_post_update_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, post.visibility)?;
    if let Some(in_reply_to) = maybe_in_reply_to {
        // TODO: disallow scope widening (see also: get_related_posts)
        validate_reply(
            &in_reply_to,
            post.author.id,
            post.visibility,
            &post_data.mentions,
        ).unwrap_or_else(|error| log::warn!("{error}"));
    };
    check_post_limits(&ap_client.limits.posts, &post_data.attachments, Remote)?;
    let (post, deletion_queue) =
        update_post(db_client, post.id, post_data).await?;
    deletion_queue.into_job(db_client).await?;
    save_attributed_object(
        db_client,
        post.expect_remote_object_id(),
        object_json,
        post.id,
    ).await?;
    Ok(post)
}

#[cfg(test)]
mod tests {
    use apx_sdk::constants::AP_PUBLIC;
    use serde_json::json;
    use mitra_models::profiles::types::DbActor;
    use super::*;

    #[test]
    fn test_deserialize_object() {
        let object_value = json!({
            "id": "https://social.example/objects/123",
            "type": "Note",
            "attributedTo": "https://social.example/users/1",
            "content": "test",
            "inReplyTo": "https://social.example/objects/121",
        });
        let object: AttributedObject =
            serde_json::from_value(object_value).unwrap();
        assert_eq!(
            object.attributed_to,
            json!("https://social.example/users/1"),
        );
        assert_eq!(object.content.unwrap(), "test");
        assert_eq!(
            object.in_reply_to.unwrap(),
            "https://social.example/objects/121",
        );
    }

    #[test]
    fn test_deserialize_object_with_attributed_to_array() {
        let object_value = json!({
            "id": "https://social.example/objects/123",
            "type": "Note",
            "attributedTo": ["https://social.example/actors/1"],
            "content": "test",
        });
        let object: AttributedObject =
            serde_json::from_value(object_value).unwrap();
        assert_eq!(object.attributed_to, "https://social.example/actors/1");
    }

    #[test]
    fn test_deserialize_object_with_attachment() {
        let object_value = json!({
            "id": "https://social.example/objects/123",
            "type": "Note",
            "attributedTo": "https://social.example/users/1",
            "content": "test",
            "attachment": {
                "type": "Image",
                "url": "https://social.example/media/image.png",
            },
        });
        let object: AttributedObject =
            serde_json::from_value(object_value).unwrap();
        assert_eq!(object.attachment.len(), 1);
        let Attachment::Media(ref media_object) = object.attachment[0] else {
            panic!();
        };
        assert_eq!(media_object.url, "https://social.example/media/image.png");
    }

    #[test]
    fn test_get_object_content() {
        let object = AttributedObject {
            content: Some("test".to_string()),
            object_type: NOTE.to_string(),
            ..Default::default()
        };
        let content = get_object_content(&object).unwrap();
        assert_eq!(content, "test");
    }

    #[test]
    fn test_get_object_content_from_video() {
        let object = AttributedObject {
            name: Some("test-name".to_string()),
            content: Some("test-content".to_string()),
            object_type: "Video".to_string(),
            url: Some(json!([{
                "type": "Link",
                "mediaType": "text/html",
                "href": "https://example.org/xyz",
            }])),
            ..Default::default()
        };
        let mut content = get_object_content(&object).unwrap();
        let object_url = get_object_url(&object).unwrap().unwrap();
        content += &create_content_link(&object_url);
        assert_eq!(
            content,
            r#"<h1>test-name</h1>test-content<p><a href="https://example.org/xyz" rel="noopener">https://example.org/xyz</a></p>"#,
        );
    }

    #[test]
    fn test_normalize_hashtag() {
        let tag = "#ActivityPub";
        let output = normalize_hashtag(tag).unwrap();

        assert_eq!(output, "activitypub");
    }

    #[test]
    fn test_normalize_audience() {
        let audience = vec![
            "https://social.example/actors/1/followers".to_owned(),
            "as:Public".to_owned(),
            "https://social.example/actors/1/followers".to_owned(),
        ];
        let normalized_audience = normalize_audience(&audience).unwrap();
        assert_eq!(normalized_audience.len(), 2);
        assert_eq!(
            normalized_audience[0].to_string(),
            "https://social.example/actors/1/followers",
        );
        assert_eq!(
            normalized_audience[1].to_string(),
            "https://www.w3.org/ns/activitystreams#Public",
        );
    }

    #[test]
    fn test_get_object_visibility_public() {
        let author =
            DbActorProfile::remote_for_test("test", "https://social.example");
        let audience = vec![AP_PUBLIC.to_string()];
        let (visibility, context) = get_object_visibility(
            &author,
            &audience,
            None,
        );
        assert_eq!(visibility, Visibility::Public);
        let PostContext::Top { audience } = context else { unreachable!() };
        assert_eq!(audience.unwrap(), AP_PUBLIC);
    }

    #[test]
    fn test_get_object_visibility_public_reply() {
        let in_reply_to_author = DbActorProfile::local_for_test("test");
        let in_reply_to = PostDetailed::local_for_test(&in_reply_to_author);
        let author =
            DbActorProfile::remote_for_test("test", "https://social.example");
        let audience = vec![AP_PUBLIC.to_string()];
        let (visibility, context) = get_object_visibility(
            &author,
            &audience,
            Some(&in_reply_to),
        );
        assert_eq!(visibility, Visibility::Public);
        assert!(matches!(context, PostContext::Reply { .. }));
    }

    #[test]
    fn test_get_object_visibility_followers() {
        let author_id = "https://example.com/users/author";
        let author_followers = "https://example.com/users/author/followers";
        let author = DbActorProfile::remote_for_test_with_data(
            "author",
            DbActor {
                id: author_id.to_string(),
                followers: Some(author_followers.to_string()),
                ..Default::default()
            },
        );
        let audience = vec![author_followers.to_string()];
        let (visibility, context) = get_object_visibility(
            &author,
            &audience,
            None,
        );
        assert_eq!(visibility, Visibility::Followers);
        let PostContext::Top { audience } = context else { unreachable!() };
        assert_eq!(audience.unwrap(), author_followers);
    }

    #[test]
    fn test_get_object_visibility_followers_reply() {
        let in_reply_to_author = DbActorProfile::local_for_test("test");
        let in_reply_to_followers = "https://social.example/users/test/followers";
        let in_reply_to = {
            let mut post = PostDetailed::local_for_test(&in_reply_to_author);
            post.visibility = Visibility::Followers;
            if let Some(ref mut conversation) = post.conversation.as_mut() {
                conversation.audience = Some(in_reply_to_followers.to_string());
            };
            post
        };
        let author_id = "https://remote.example/users/author";
        let author = DbActorProfile::remote_for_test("author", author_id);
        let audience = vec![in_reply_to_followers.to_string()];
        let (visibility, context) = get_object_visibility(
            &author,
            &audience,
            Some(&in_reply_to),
        );
        assert_eq!(visibility, Visibility::Conversation);
        assert!(matches!(context, PostContext::Reply { .. }));
    }

    #[test]
    fn test_get_object_visibility_followers_reply_from_mastodon() {
        let in_reply_to_author = DbActorProfile::local_for_test("test");
        let in_reply_to_followers = "https://social.example/users/test/followers";
        let in_reply_to = {
            let mut post = PostDetailed::local_for_test(&in_reply_to_author);
            post.visibility = Visibility::Followers;
            if let Some(ref mut conversation) = post.conversation.as_mut() {
                conversation.audience = Some(in_reply_to_followers.to_string());
            };
            post
        };
        let author_id = "https://remote.example/users/author";
        let author = DbActorProfile::remote_for_test("author", author_id);
        let author_followers = author
            .expect_actor_data()
            .followers.clone().unwrap();
        let audience = vec![author_followers];
        let (visibility, context) = get_object_visibility(
            &author,
            &audience,
            Some(&in_reply_to),
        );
        assert_eq!(visibility, Visibility::Conversation);
        assert!(matches!(context, PostContext::Reply { .. }));
    }

    #[test]
    fn test_get_object_visibility_subscribers() {
        let author_id = "https://example.com/users/author";
        let author_followers = "https://example.com/users/author/followers";
        let author_subscribers = "https://example.com/users/author/subscribers";
        let author = DbActorProfile::remote_for_test_with_data(
            "author",
            DbActor {
                id: author_id.to_string(),
                followers: Some(author_followers.to_string()),
                subscribers: Some(author_subscribers.to_string()),
                ..Default::default()
            },
        );
        let audience = vec![author_subscribers.to_string()];
        let (visibility, context) = get_object_visibility(
            &author,
            &audience,
            None,
        );
        assert_eq!(visibility, Visibility::Subscribers);
        let PostContext::Top { audience } = context else { unreachable!() };
        assert_eq!(audience.unwrap(), author_subscribers);
    }

    #[test]
    fn test_get_object_visibility_direct() {
        let author = DbActorProfile::remote_for_test("test", "https://x.example");
        let audience = vec!["https://example.com/users/1".to_string()];
        let (visibility, context) = get_object_visibility(
            &author,
            &audience,
            None,
        );
        assert_eq!(visibility, Visibility::Direct);
        let PostContext::Top { audience } = context else { unreachable!() };
        assert!(audience.is_none());
    }
}
