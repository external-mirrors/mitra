use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{
    Deserialize,
    Deserializer,
    de::{Error as DeserializerError},
};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use apx_sdk::{
    addresses::WebfingerAddress,
    constants::{AP_MEDIA_TYPE, AS_MEDIA_TYPE},
    deserialization::{
        deserialize_into_id_array,
        deserialize_into_link_href,
        deserialize_into_object_id_opt,
        deserialize_object_array,
        parse_into_href_array,
    },
    fetch::fetch_file,
    url::is_same_origin,
    utils::is_public,
};
use mitra_adapters::permissions::filter_mentions;
use mitra_models::{
    activitypub::queries::save_attributed_object,
    attachments::queries::create_attachment,
    database::{DatabaseClient, DatabaseError},
    filter_rules::types::FilterAction,
    media::types::MediaInfo,
    posts::{
        queries::{create_post, update_post},
        types::{
            Post,
            PostContext,
            PostCreateData,
            PostUpdateData,
            Visibility,
        },
    },
    profiles::types::DbActorProfile,
};
use mitra_utils::{
    files::FileInfo,
    html::clean_html,
};
use mitra_validators::{
    errors::ValidationError,
    media::{validate_media_description, validate_media_url},
    posts::{
        clean_title,
        content_allowed_classes,
        validate_post_create_data,
        validate_post_mentions,
        validate_post_update_data,
        ATTACHMENT_LIMIT,
        CONTENT_MAX_SIZE,
        EMOJI_LIMIT,
        HASHTAG_LIMIT,
        LINK_LIMIT,
        MENTION_LIMIT,
    },
    tags::validate_hashtag,
};

use crate::{
    agent::build_federation_agent,
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
    attributed_to: JsonValue,

    name: Option<String>,
    content: Option<String>,
    media_type: Option<String>,
    pub sensitive: Option<bool>,
    summary: Option<String>,

    #[serde(
        default,
        deserialize_with = "deserialize_object_array",
    )]
    icon: Vec<JsonValue>,

    #[serde(
        default,
        deserialize_with = "deserialize_object_array",
    )]
    attachment: Vec<JsonValue>,

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

    published: Option<DateTime<Utc>>,
    pub updated: Option<DateTime<Utc>>,
    url: Option<JsonValue>,

    // Polls
    one_of: Option<JsonValue>,
    any_of: Option<JsonValue>,

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

    pub fn in_reply_to(&self) -> Option<&str> {
        self.inner.in_reply_to.as_deref()
    }

    pub fn links(&self) -> Vec<String> {
        get_object_links(&self.inner)
    }
}

pub(super) fn get_object_attributed_to(object: &AttributedObject)
    -> Result<String, ValidationError>
{
    parse_attributed_to(&object.attributed_to)
}

fn get_object_url(object: &AttributedObject)
    -> Result<String, ValidationError>
{
    let maybe_object_url = match &object.url {
        Some(value) => {
            let links = parse_into_href_array(value)
                .map_err(|_| ValidationError("invalid object URL"))?;
            links.into_iter().next()
        },
        None => None,
    };
    let object_url = maybe_object_url.unwrap_or(object.id.clone());
    Ok(object_url)
}

/// Get post content by concatenating name/summary and content
fn get_object_content(object: &AttributedObject) ->
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
            content.to_string()
        }
    } else {
        "".to_string()
    };
    let content = format!("{}{}", title, content);
    if content.len() > CONTENT_MAX_SIZE {
        return Err(ValidationError("content is too long"));
    };
    let content_safe = clean_html(&content, content_allowed_classes());
    Ok(content_safe)
}

fn create_content_link(url: String) -> String {
    format!(
        r#"<p><a href="{0}" rel="noopener">{0}</a></p>"#,
        url,
    )
}

fn is_gnu_social_link(author_id: &str, attachment: &Attachment) -> bool {
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attachment {
    #[serde(rename = "type")]
    attachment_type: String,

    name: Option<String>,
    media_type: Option<String>,

    #[serde(deserialize_with = "deserialize_into_link_href")]
    url: String,
}

async fn get_object_attachments(
    ap_client: &ApClient,
    db_client: &impl DatabaseClient,
    object: &AttributedObject,
    author: &DbActorProfile,
) -> Result<(Vec<Uuid>, Vec<String>), HandlerError> {
    let agent = build_federation_agent(&ap_client.instance, None);
    let author_hostname = get_moderation_domain(author.expect_actor_data())?;
    let is_filter_enabled = ap_client.filter.is_action_required(
        author_hostname.as_str(),
        FilterAction::RejectMediaAttachments,
    );

    let mut values = object.attachment.clone();
    if object.object_type == VIDEO {
        // PeerTube video thumbnails
        values.extend(object.icon.iter().take(1).cloned());
    };
    let mut attachments = vec![];
    let mut unprocessed = vec![];
    let mut downloaded: Vec<(String, FileInfo, Option<String>)> = vec![];
    for attachment_value in values {
        // Stop downloading if limit is reached
        if downloaded.len() >= ATTACHMENT_LIMIT {
            log::warn!("too many attachments");
            break;
        };
        match attachment_value["type"].as_str() {
            Some(AUDIO | DOCUMENT | IMAGE | VIDEO) => (),
            Some(LINK) => {
                // Lemmy compatibility
                let link_href = if let Some(href) = attachment_value["href"].as_str() {
                    href.to_string()
                } else {
                    log::warn!("invalid link attachment");
                    continue;
                };
                unprocessed.push(link_href);
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
        let attachment: Attachment =
            match serde_json::from_value(attachment_value)
        {
            Ok(attachment) => attachment,
            Err(error) => {
                log::warn!("invalid attachment ({error})");
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
        if downloaded.iter().any(|(url, ..)| *url == attachment_url) {
            // Already downloaded
            log::warn!("skipping duplicate attachment: {attachment_url}");
            continue;
        };
        let maybe_description = attachment.name.filter(|name| {
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
        let (file_data, media_type) = match fetch_file(
            &agent,
            &attachment_url,
            attachment.media_type.as_deref(),
            &ap_client.media_limits.supported_media_types(),
            ap_client.media_limits.file_size_limit,
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
        let file_info =
            ap_client.media_storage.save_file(file_data, &media_type)?;
        log::info!("downloaded attachment {}", attachment_url);
        downloaded.push((attachment_url, file_info, maybe_description));
    };
    for (attachment_url, file_info, description) in downloaded {
        let db_attachment = create_attachment(
            db_client,
            author.id,
            MediaInfo::remote(file_info, attachment_url),
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
    if let Some(ref object_id) = object.quote_url {
        if !links.contains(object_id) {
            links.push(object_id.to_owned());
        };
    };
    links
}

async fn get_object_tags(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
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
                    db_client,
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
                    db_client,
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
                db_client,
                &instance.url(),
                &canonical_linked_id,
            ).await?;
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
                db_client,
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
    let audience = get_audience(object);
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
            &instance.url(),
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
    if let Some(ref object_id) = object.quote_url {
        let object_id = redirects.get(object_id).unwrap_or(object_id);
        let canonical_object_id = canonicalize_id(object_id)?;
        let linked = get_post_by_object_id(
            db_client,
            &instance.url(),
            &canonical_object_id,
        ).await?;
        if links.len() < LINK_LIMIT && !links.contains(&linked.id) {
            links.push(linked.id);
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

pub(super) fn get_audience(object: &AttributedObject) -> Vec<String> {
    [object.to.clone(), object.cc.clone()].concat()
}

fn get_object_visibility(
    author: &DbActorProfile,
    audience: &[String],
) -> Visibility {
    if audience.iter().any(is_public) {
        return Visibility::Public;
    };
    let actor = author.expect_actor_data();
    if let Some(ref followers) = actor.followers {
        if audience.contains(followers) {
            return Visibility::Followers;
        };
    };
    if let Some(ref subscribers) = actor.subscribers {
        if audience.contains(subscribers) {
            return Visibility::Subscribers;
        };
    };
    log::info!(
        "processing note with visibility 'direct' attributed to {}",
        actor.id,
    );
    Visibility::Direct
}

fn parse_poll_results(
    object: &AttributedObject,
) -> Result<String, ValidationError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Replies {
        total_items: u32,
    }
    #[derive(Deserialize)]
    struct PollOption {
        name: String,
        replies: Replies,
    }

    let poll = if let Some(ref value) = object.one_of {
        // Single choice
        value
    } else {
        // Multiple choices
        object.any_of.as_ref()
            .ok_or(ValidationError("poll results are not present"))?
    };

    let values = poll
        .as_array()
        .ok_or(ValidationError("invalid poll results"))?;
    if values.is_empty() {
        return Err(ValidationError("poll is empty"));
    };

    let mut poll_results = "".to_string();
    for option_value in values {
        let option: PollOption = serde_json::from_value(option_value.clone())
            .map_err(|_| ValidationError("invalid poll result"))?;
        let option_text = format!(
            r#"<p>{0}: {1}</p>"#,
            option.name,
            option.replies.total_items,
        );
        poll_results += &option_text;
    };
    Ok(poll_results)
}

pub async fn create_remote_post(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    object: AttributedObjectJson,
    redirects: &HashMap<String, String>,
) -> Result<Post, HandlerError> {
    let AttributedObjectJson { inner: object, value: object_value } = object;
    let canonical_object_id = canonicalize_id(&object.id)?;

    object.check_not_actor()?;
    if object.object_type != NOTE {
        // Attempting to convert any object that has attributedTo property
        // into post
        log::info!("processing object of type {}", object.object_type);
    };

    let author_id = get_object_attributed_to(&object)?;
    if !is_same_origin(&author_id, &object.id)
        .map_err(|_| ValidationError("invalid object ID"))?
    {
        return Err(ValidationError("object attributed to actor from different server").into());
    };
    let instance = &ap_client.instance;
    let author = ActorIdResolver::default().only_remote().resolve(
        ap_client,
        db_client,
        &author_id,
    ).await.map_err(|err| {
        log::warn!("failed to import {} ({})", author_id, err);
        err
    })?;

    let mut content = get_object_content(&object)?;
    if object.object_type == QUESTION {
        match parse_poll_results(&object) {
            Ok(poll_results) => content += &poll_results,
            Err(error) => log::warn!("{error}"),
        };
    };
    if object.object_type != NOTE {
        // Append link to object
        let object_url = get_object_url(&object)?;
        content += &create_content_link(object_url);
    };
    let (attachments, unprocessed) = get_object_attachments(
        ap_client,
        db_client,
        &object,
        &author,
    ).await?;
    for attachment_url in unprocessed {
        content += &create_content_link(attachment_url);
    };

    let (mentions, hashtags, links, emojis) = get_object_tags(
        ap_client,
        db_client,
        &object,
        &author,
        redirects,
    ).await?;

    let maybe_in_reply_to = match object.in_reply_to {
        Some(ref object_id) => {
            let object_id = redirects.get(object_id).unwrap_or(object_id);
            let canonical_in_reply_to_id = canonicalize_id(object_id)?;
            let in_reply_to = get_post_by_object_id(
                db_client,
                &instance.url(),
                &canonical_in_reply_to_id,
            ).await?;
            Some(in_reply_to)
        },
        None => None,
    };

    // TODO: use on local posts too
    let mentions = filter_mentions(
        db_client,
        mentions,
        &author,
        maybe_in_reply_to.as_ref().map(|post| post.id),
    ).await?;

    let audience = get_audience(&object);
    let context = match maybe_in_reply_to {
        Some(ref in_reply_to) => PostContext::Reply { in_reply_to_id: in_reply_to.id },
        None => PostContext::Top,
    };
    let visibility = get_object_visibility(&author, &audience);
    let is_sensitive = object.sensitive.unwrap_or(false);
    let created_at = object.published.unwrap_or(Utc::now());

    if visibility == Visibility::Direct &&
        !mentions.iter().any(|profile| profile.is_local())
    {
        log::warn!("direct message has no local recipients");
    };

    let post_data = PostCreateData {
        context: context,
        content: content,
        content_source: None,
        visibility,
        is_sensitive,
        attachments: attachments,
        mentions: mentions.iter().map(|profile| profile.id).collect(),
        tags: hashtags,
        links: links,
        emojis: emojis,
        object_id: Some(canonical_object_id.to_string()),
        created_at,
    };
    validate_post_create_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, post_data.visibility)?;
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
    db_client: &mut impl DatabaseClient,
    post: Post,
    object: &AttributedObjectJson,
) -> Result<(), HandlerError> {
    assert!(!post.is_local());
    let AttributedObjectJson { inner: object, value: object_json } = object;
    let author_id = get_object_attributed_to(object)?;
    let canonical_author_id = canonicalize_id(&author_id)?;
    if canonical_author_id.to_string() != post.author.expect_remote_actor_id() {
        return Err(ValidationError("object owner can't be changed").into());
    };
    let mut content = get_object_content(object)?;
    if object.object_type == QUESTION {
        match parse_poll_results(object) {
            Ok(poll_results) => content += &poll_results,
            Err(error) => log::warn!("{error}"),
        };
    };
    if object.object_type != NOTE {
        // Append link to object
        let object_url = get_object_url(object)?;
        content += &create_content_link(object_url);
    };
    let (attachments, unprocessed) = get_object_attachments(
        ap_client,
        db_client,
        object,
        &post.author,
    ).await?;
    for attachment_url in unprocessed {
        content += &create_content_link(attachment_url);
    };
    let (mentions, hashtags, links, emojis) = get_object_tags(
        ap_client,
        db_client,
        object,
        &post.author,
        &HashMap::new(),
    ).await?;
    let is_sensitive = object.sensitive.unwrap_or(false);
    let updated_at = object.updated.unwrap_or(Utc::now());

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

    let post_data = PostUpdateData {
        content,
        content_source: None,
        is_sensitive,
        attachments,
        mentions: mentions.iter().map(|profile| profile.id).collect(),
        tags: hashtags,
        links,
        emojis,
        updated_at,
    };
    validate_post_update_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, post.visibility)?;
    let (_, deletion_queue) =
        update_post(db_client, post.id, post_data).await?;
    deletion_queue.into_job(db_client).await?;
    save_attributed_object(
        db_client,
        &post.object_id.expect("object ID should be present"),
        object_json,
        post.id,
    ).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use apx_sdk::constants::AP_PUBLIC;
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
    fn test_get_object_attributed_to() {
       let object = AttributedObject {
            object_type: NOTE.to_string(),
            attributed_to: json!(["https://example.org/1"]),
            ..Default::default()
        };
        let author_id = get_object_attributed_to(&object).unwrap();
        assert_eq!(author_id, "https://example.org/1");
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
        let object_url = get_object_url(&object).unwrap();
        content += &create_content_link(object_url);
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
    fn test_get_object_visibility_public() {
        let author =
            DbActorProfile::remote_for_test("test", "https://social.example");
        let audience = vec![AP_PUBLIC.to_string()];
        let visibility = get_object_visibility(&author, &audience);
        assert_eq!(visibility, Visibility::Public);
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
        let visibility = get_object_visibility(&author, &audience);
        assert_eq!(visibility, Visibility::Followers);
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
        let visibility = get_object_visibility(&author, &audience);
        assert_eq!(visibility, Visibility::Subscribers);
    }

    #[test]
    fn test_get_object_visibility_direct() {
        let author = DbActorProfile::remote_for_test("test", "https://x.example");
        let audience = vec!["https://example.com/users/1".to_string()];
        let visibility = get_object_visibility(&author, &audience);
        assert_eq!(visibility, Visibility::Direct);
    }
}
