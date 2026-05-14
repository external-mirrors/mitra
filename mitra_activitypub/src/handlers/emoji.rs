use apx_core::url::{
    http_uri::Hostname,
    http_url_whatwg::get_hostname,
};
use apx_sdk::fetch::fetch_media;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_models::{
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    emojis::queries::{
        create_or_update_remote_emoji,
        get_remote_emoji_by_object_id,
        update_emoji,
    },
    emojis::types::{CustomEmoji as DbCustomEmoji},
    filter_rules::types::FilterAction,
    media::types::MediaInfo,
    profiles::types::Origin::Remote,
};
use mitra_validators::{
    activitypub::validate_object_id,
    emojis::{
        clean_emoji_name,
        validate_emoji_name,
        EMOJI_REMOTE_MEDIA_TYPES,
    },
    media::validate_media_url,
    profiles::validate_hostname,
    errors::ValidationError,
};

use crate::{
    importers::ApClient,
};

use super::HandlerError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmojiImage {
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Emoji {
    id: Option<String>,
    name: String,
    alternate_name: Option<String>,
    icon: EmojiImage,
    #[serde(default)]
    updated: DateTime<Utc>,
}

// Returns None if emoji is not valid or when fetcher fails.
// Returns HandlerError on database and filesystem errors.
pub async fn handle_emoji(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    moderation_domain: &Hostname,
    tag_value: JsonValue,
) -> Result<Option<DbCustomEmoji>, HandlerError> {
    let emoji: Emoji = match serde_json::from_value(tag_value) {
        Ok(emoji) => emoji,
        Err(error) => {
            log::warn!("invalid emoji tag: {}", error);
            return Ok(None);
        },
    };
    // Akkoma uses anonymous Emojis
    // https://akkoma.dev/AkkomaGang/akkoma/pulls/815
    let emoji_object_id = emoji.id.unwrap_or(emoji.icon.url.clone());
    if validate_object_id(&emoji_object_id).is_err() {
        log::warn!("invalid emoji ID: {}", emoji_object_id);
        return Ok(None);
    };
    let emoji_name = clean_emoji_name(&emoji.name);
    if validate_emoji_name(emoji_name, Remote).is_err() {
        log::warn!("invalid emoji name: {}", emoji_name);
        return Ok(None);
    };
    if let Some(alternate_name) = emoji.alternate_name {
        log::warn!("alternate name for {emoji_name}:  {alternate_name}");
    };
    let emoji_hostname = match get_hostname(&emoji_object_id)
        .map_err(|_| ValidationError("invalid emoji ID"))
        .and_then(|value| validate_hostname(&value).map(|()| value))
    {
        Ok(hostname) => hostname,
        Err(error) => {
            log::warn!("skipping emoji: {error}");
            return Ok(None);
        },
    };
    if let Err(error) = validate_media_url(&emoji.icon.url) {
        log::warn!("invalid emoji URL ({error}): {}", emoji.icon.url);
        return Ok(None);
    };
    let is_filter_enabled = ap_client.filter.is_action_required(
        moderation_domain.as_str(),
        FilterAction::RejectCustomEmojis,
    );
    if is_filter_enabled {
        log::warn!("emoji removed by filter: {}", emoji_object_id);
        return Ok(None);
    };
    let maybe_emoji_id = match get_remote_emoji_by_object_id(
        db_client_await!(db_pool),
        &emoji_object_id,
    ).await {
        Ok(db_emoji) => {
            if db_emoji.updated_at >= emoji.updated {
                // Emoji already exists and is up to date
                return Ok(Some(db_emoji));
            };
            if db_emoji.emoji_name != emoji_name {
                log::warn!("emoji name can't be changed");
                return Ok(None);
            };
            Some(db_emoji.id)
        },
        Err(DatabaseError::NotFound("emoji")) => None,
        Err(other_error) => return Err(other_error.into()),
    };
    let (file_data, media_type) = match fetch_media(
        &ap_client.agent(),
        &emoji.icon.url,
        &EMOJI_REMOTE_MEDIA_TYPES,
        ap_client.limits.media.emoji_size_limit,
    ).await {
        Ok(file) => file,
        Err(error) => {
            log::warn!("failed to fetch emoji: {}", error);
            return Ok(None);
        },
    };
    let is_proxy_enabled = ap_client.filter.is_action_required(
        moderation_domain.as_str(),
        FilterAction::ProxyMedia,
    );
    let image = if is_proxy_enabled {
        log::info!("linked emoji {}", emoji.icon.url);
        MediaInfo::link(media_type, emoji.icon.url)
    } else {
        let file_info = ap_client.media_storage
            .save_file(file_data, &media_type)?;
        log::info!("downloaded emoji {}", emoji.icon.url);
        MediaInfo::remote(file_info, emoji.icon.url)
    };
    let db_client = &mut **get_database_client(db_pool).await?;
    let db_emoji = if let Some(emoji_id) = maybe_emoji_id {
        let (db_emoji, deletion_queue) = update_emoji(
            db_client,
            emoji_id,
            image,
            emoji.updated,
        ).await?;
        deletion_queue.into_job(db_client).await?;
        db_emoji
    } else {
        // Handles emoji name conflicts
        let (db_emoji, deletion_queue) = create_or_update_remote_emoji(
            db_client,
            emoji_name,
            &emoji_hostname,
            image,
            &emoji_object_id,
            emoji.updated,
        ).await?;
        deletion_queue.into_job(db_client).await?;
        db_emoji
    };
    Ok(Some(db_emoji))
}
