use apx_sdk::{
    constants::AP_PUBLIC,
    deserialization::{
        deserialize_into_id_array,
        deserialize_into_object_id,
        deserialize_object_array,
    },
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    posts::types::Visibility,
    profiles::types::DbActor,
    reactions::{
        queries::create_reaction,
        types::ReactionData,
    },
};
use mitra_utils::unicode::is_single_character;
use mitra_validators::{
    errors::ValidationError,
    reactions::validate_reaction_data,
};

use crate::{
    builders::add_context_activity::sync_conversation,
    filter::get_moderation_domain,
    identifiers::canonicalize_id,
    importers::{
        get_post_by_object_id,
        ActorIdResolver,
        ApClient,
    },
    vocabulary::DISLIKE,
};

use super::{
    emoji::handle_emoji,
    note::normalize_audience,
    Descriptor,
    HandlerResult,
};

#[derive(Deserialize)]
struct Like {
    id: String,

    #[serde(rename = "type")]
    activity_type: String,

    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,

    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,

    content: Option<String>,

    #[serde(default, deserialize_with = "deserialize_object_array")]
    tag: Vec<JsonValue>,

    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    to: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_into_id_array")]
    cc: Vec<String>,
}

fn get_visibility(
    actor_data: &DbActor,
    to: &[String],
    cc: &[String],
) -> Result<Visibility, ValidationError> {
    let audience = normalize_audience(&[to, cc].concat())?;
    let visibility = if audience.iter()
        .any(|target_id| target_id.to_string() == AP_PUBLIC)
    {
        Visibility::Public
    } else if audience.iter()
        .any(|target_id| Some(target_id.to_string()) == actor_data.followers)
    {
        log::warn!("followers-only reaction converted to direct");
        Visibility::Direct
    } else {
        Visibility::Direct
    };
    Ok(visibility)
}

pub async fn handle_like(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let like: Like = serde_json::from_value(activity.clone())?;
    let ap_client = ApClient::new_with_pool(config, db_pool).await?;
    let instance = &ap_client.instance;
    let author = ActorIdResolver::default().only_remote().resolve(
        &ap_client,
        db_pool,
        &like.actor,
    ).await?;
    let canonical_object_id = canonicalize_id(&like.object)?;
    let post = match get_post_by_object_id(
        db_client_await!(db_pool),
        instance.uri_str(),
        &canonical_object_id,
    ).await {
        Ok(post) => post,
        // Ignore like if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let visibility = get_visibility(
        author.expect_actor_data(),
        &like.to,
        &like.cc,
    )?;
    let (maybe_content, maybe_emoji_id) = match like.content {
        Some(content) if is_single_character(&content) => {
            (Some(content), None)
        },
        Some(content) => {
            let maybe_db_emoji = if let Some(emoji_value) = like.tag.first() {
                let moderation_domain =
                    get_moderation_domain(author.expect_actor_data())?;
                let maybe_db_emoji = handle_emoji(
                    &ap_client,
                    db_pool,
                    &moderation_domain,
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
            if like.activity_type == DISLIKE {
                // Transform Dislike activity into emoji reaction
                (Some("👎".to_string()), None)
            } else {
                (None, None)
            }
        },
    };
    let canonical_activity_id = canonicalize_id(&like.id)?;
    let reaction_data = ReactionData {
        author_id: author.id,
        post_id: post.id,
        content: maybe_content,
        emoji_id: maybe_emoji_id,
        visibility: visibility,
        activity_id: Some(canonical_activity_id.to_string()),
    };
    validate_reaction_data(&reaction_data)?;
    let db_client = &mut **get_database_client(db_pool).await?;
    match create_reaction(db_client, reaction_data).await {
        Ok(reaction) => {
            sync_conversation(
                db_client,
                instance,
                post.expect_conversation(),
                activity,
                reaction.visibility,
            ).await?;
        },
        // Ignore activity if reaction is already saved
        Err(DatabaseError::AlreadyExists(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(Descriptor::object("Object")))
}
