use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_federation::deserialization::deserialize_into_object_id;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    reactions::queries::create_reaction,
};
use mitra_services::media::MediaStorage;
use mitra_validators::errors::ValidationError;

use crate::activitypub::{
    importers::{
        get_post_by_object_id,
        ActorIdResolver,
    },
    vocabulary::NOTE,
};

use super::HandlerResult;

#[derive(Deserialize)]
struct Like {
    id: String,
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
    content: Option<String>,
}

pub async fn handle_like(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity_value: JsonValue,
) -> HandlerResult {
    let activity: Like = serde_json::from_value(activity_value.clone())
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let author = ActorIdResolver::default().only_remote().resolve(
        db_client,
        &config.instance(),
        &MediaStorage::from(config),
        &activity.actor,
    ).await?;
    let post_id = match get_post_by_object_id(
        db_client,
        &config.instance_url(),
        &activity.object,
    ).await {
        Ok(post) => post.id,
        // Ignore like if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    match create_reaction(
        db_client,
        &author.id,
        &post_id,
        Some(&activity.id),
    ).await {
        Ok(_) => (),
        // Ignore activity if reaction is already saved
        Err(DatabaseError::AlreadyExists(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    // TODO: fetch custom emojis
    if activity.content.is_some() {
        log::info!("reaction with content: {}", activity_value);
    };
    Ok(Some(NOTE))
}
