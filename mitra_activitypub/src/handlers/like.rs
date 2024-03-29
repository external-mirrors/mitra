use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_federation::deserialization::{
    deserialize_into_object_id,
    deserialize_object_array,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    reactions::{
        queries::create_reaction,
        types::ReactionData,
    },
};
use mitra_services::media::MediaStorage;
use mitra_utils::unicode::is_single_character;
use mitra_validators::{
    errors::ValidationError,
    reactions::validate_reaction_data,
};

use crate::{
    agent::build_federation_agent,
    importers::{
        get_post_by_object_id,
        ActorIdResolver,
    },
    vocabulary::{DISLIKE, NOTE},
};

use super::{
    emoji::handle_emoji,
    HandlerResult,
};

#[derive(Deserialize)]
struct Like {
    id: String,

    #[serde(rename = "type")]
    activity_type: String,

    actor: String,

    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,

    content: Option<String>,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    tag: Vec<JsonValue>,
}

pub async fn handle_like(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity_value: JsonValue,
) -> HandlerResult {
    let activity: Like = serde_json::from_value(activity_value)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let instance = config.instance();
    let agent = build_federation_agent(&instance, None);
    let storage = MediaStorage::from(config);
    let author = ActorIdResolver::default().only_remote().resolve(
        db_client,
        &instance,
        &storage,
        &activity.actor,
    ).await?;
    let post_id = match get_post_by_object_id(
        db_client,
        &instance.url(),
        &activity.object,
    ).await {
        Ok(post) => post.id,
        // Ignore like if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let (maybe_content, maybe_emoji_id) = match activity.content {
        Some(content) if is_single_character(&content) => {
            (Some(content), None)
        },
        Some(content) => {
            let maybe_db_emoji = if let Some(emoji_value) = activity.tag.first() {
                let maybe_db_emoji = handle_emoji(
                    &agent,
                    db_client,
                    &storage,
                    emoji_value.clone(),
                ).await?;
                // Emoji shortcode must match content
                maybe_db_emoji
                    .filter(|emoji| emoji.shortcode() == content)
            } else {
                None
            };
            if let Some(db_emoji) = maybe_db_emoji {
                (Some(content), Some(db_emoji.id))
            } else {
                log::warn!("invalid custom emoji reaction");
                return Ok(None);
            }
        },
        None => {
            if activity.activity_type == DISLIKE {
                // Transform Dislike activity into emoji reaction
                (Some("👎".to_string()), None)
            } else {
                (None, None)
            }
        },
    };
    let reaction_data = ReactionData {
        author_id: author.id,
        post_id: post_id,
        content: maybe_content,
        emoji_id: maybe_emoji_id,
        activity_id: Some(activity.id),
    };
    validate_reaction_data(&reaction_data)?;
    match create_reaction(db_client, reaction_data).await {
        Ok(_) => (),
        // Ignore activity if reaction is already saved
        Err(DatabaseError::AlreadyExists(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(NOTE))
}
