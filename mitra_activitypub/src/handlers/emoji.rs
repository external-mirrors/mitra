use serde_json::{Value as JsonValue};

use mitra_federation::{
    agent::FederationAgent,
    fetch::fetch_file,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    emojis::queries::{
        create_emoji,
        get_emoji_by_remote_object_id,
        update_emoji,
    },
    emojis::types::{DbEmoji, EmojiImage},
};
use mitra_services::media::MediaStorage;
use mitra_utils::urls::get_hostname;
use mitra_validators::{
    emojis::{
        validate_emoji_name,
        EMOJI_MEDIA_TYPES,
    },
    profiles::validate_hostname,
    errors::ValidationError,
};

use crate::builders::emoji::Emoji;

use super::HandlerError;

fn parse_emoji_shortcode(shortcode: &str) -> Result<&str, ValidationError> {
    shortcode.strip_prefix(':')
        .and_then(|val| val.strip_suffix(':'))
        .ok_or(ValidationError("invalid emoji shortcode"))
}

// Returns None if emoji is not valid or when fetcher fails.
// Returns HandlerError on database and filesystem errors.
pub async fn handle_emoji(
    agent: &FederationAgent,
    db_client: &impl DatabaseClient,
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
    let emoji_name = parse_emoji_shortcode(&emoji.name)?;
    if validate_emoji_name(emoji_name).is_err() {
        log::warn!("invalid emoji name: {}", emoji_name);
        return Ok(None);
    };
    let maybe_emoji_id = match get_emoji_by_remote_object_id(
        db_client,
        &emoji.id,
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
    let (file_data, file_size, media_type) = match fetch_file(
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
    let file_name = storage.save_file(file_data, &media_type)?;
    log::info!("downloaded emoji {}", emoji.icon.url);
    let image = EmojiImage { file_name, file_size, media_type };
    let db_emoji = if let Some(emoji_id) = maybe_emoji_id {
        update_emoji(
            db_client,
            emoji_id,
            image,
            emoji.updated,
        ).await?
    } else {
        let hostname = match get_hostname(&emoji.id)
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
            Some(&emoji.id),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_emoji_shortcode() {
        let shortcode = ":test:";
        let emoji_name = parse_emoji_shortcode(shortcode).unwrap();
        assert_eq!(emoji_name, "test");
    }
}
