use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_core::urls::get_hostname;
use apx_sdk::{
    agent::FederationAgent,
    fetch::fetch_file,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    emojis::queries::{
        create_emoji,
        get_remote_emoji_by_object_id,
        update_emoji,
    },
    emojis::types::{DbEmoji, EmojiImage as DbEmojiImage},
    media::types::MediaInfo,
};
use mitra_services::media::MediaStorage;
use mitra_validators::{
    activitypub::validate_object_id,
    emojis::{
        clean_emoji_name,
        validate_emoji_name,
        EMOJI_MEDIA_TYPES,
    },
    media::validate_media_url,
    profiles::validate_hostname,
    errors::ValidationError,
};

use super::HandlerError;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmojiImage {
    url: String,
    media_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Emoji {
    id: Option<String>,
    name: String,
    icon: EmojiImage,
    #[serde(default)]
    updated: DateTime<Utc>,
}

// Returns None if emoji is not valid or when fetcher fails.
// Returns HandlerError on database and filesystem errors.
pub async fn handle_emoji(
    agent: &FederationAgent,
    db_client: &mut impl DatabaseClient,
    storage: &MediaStorage,
    tag_value: JsonValue,
) -> Result<Option<DbEmoji>, HandlerError> {
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
    if validate_emoji_name(emoji_name).is_err() {
        log::warn!("invalid emoji name: {}", emoji_name);
        return Ok(None);
    };
    let maybe_emoji_id = match get_remote_emoji_by_object_id(
        db_client,
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
    if let Err(error) = validate_media_url(&emoji.icon.url) {
        log::warn!("invalid emoji URL ({error}): {}", emoji.icon.url);
        return Ok(None);
    };
    let (file_data, media_type) = match fetch_file(
        agent,
        &emoji.icon.url,
        emoji.icon.media_type.as_deref(),
        &EMOJI_MEDIA_TYPES,
        storage.emoji_size_limit,
    ).await {
        Ok(file) => file,
        Err(error) => {
            log::warn!("failed to fetch emoji: {}", error);
            return Ok(None);
        },
    };
    let file_info = storage.save_file(file_data, &media_type)?;
    log::info!("downloaded emoji {}", emoji.icon.url);
    let image = DbEmojiImage::from(MediaInfo::remote(file_info, emoji.icon.url));
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
        let hostname = match get_hostname(&emoji_object_id)
            .map_err(|_| ValidationError("invalid emoji ID"))
            .and_then(|value| validate_hostname(&value).map(|()| value))
        {
            Ok(hostname) => hostname,
            Err(error) => {
                log::warn!("skipping emoji: {error}");
                return Ok(None);
            },
        };
        match create_emoji(
            db_client,
            emoji_name,
            Some(&hostname),
            image,
            Some(&emoji_object_id),
            emoji.updated,
        ).await {
            Ok(db_emoji) => db_emoji,
            Err(DatabaseError::AlreadyExists(_)) => {
                log::warn!("emoji name is not unique: {}", emoji_name);
                return Ok(None);
            },
            Err(other_error) => return Err(other_error.into()),
        }
    };
    Ok(Some(db_emoji))
}
