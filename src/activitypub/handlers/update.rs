use std::collections::HashMap;

use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::{
        get_post_by_remote_object_id,
        update_post,
    },
    posts::types::PostUpdateData,
    profiles::queries::get_profile_by_remote_actor_id,
};
use mitra_validators::{
    errors::ValidationError,
    posts::{
        validate_post_mentions,
        validate_post_update_data,
    },
};

use crate::activitypub::{
    actors::{
        helpers::update_remote_profile,
        types::Actor,
    },
    deserialization::{deserialize_into_object_id, find_object_id},
    fetcher::fetchers::fetch_object,
    handlers::create::{
        create_content_link,
        get_object_attachments,
        get_object_content,
        get_object_tags,
        get_object_url,
    },
    identifiers::profile_actor_id,
    types::Object,
    vocabulary::{ARTICLE, GROUP, NOTE, PERSON},
};
use crate::media::MediaStorage;

use super::HandlerResult;

#[derive(Deserialize)]
struct UpdateNote {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: Object,
}

async fn handle_update_note(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let activity: UpdateNote = serde_json::from_value(activity)
        .map_err(|_| ValidationError("invalid object"))?;
    let object = activity.object;
    let post = match get_post_by_remote_object_id(
        db_client,
        &object.id,
    ).await {
        Ok(post) => post,
        // Ignore Update if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let instance = config.instance();
    if profile_actor_id(&instance.url(), &post.author) != activity.actor {
        return Err(ValidationError("actor is not an author").into());
    };
    let mut content = get_object_content(&object)?;
    if object.object_type != NOTE {
        // Append link to object
        let object_url = get_object_url(&object)?;
        content += &create_content_link(object_url);
    };
    let storage = MediaStorage::from(config);
    let (attachments, unprocessed) = get_object_attachments(
        db_client,
        &instance,
        &storage,
        &object,
        &post.author,
    ).await?;
    for attachment_url in unprocessed {
        content += &create_content_link(attachment_url);
    };
    let (mentions, hashtags, links, emojis) = get_object_tags(
        db_client,
        &instance,
        &storage,
        &object,
        &HashMap::new(),
    ).await?;
    let is_sensitive = object.sensitive.unwrap_or(false);
    let updated_at = object.updated.unwrap_or(Utc::now());
    let post_data = PostUpdateData {
        content,
        content_source: None,
        is_sensitive,
        attachments,
        mentions,
        tags: hashtags,
        links,
        emojis,
        updated_at,
    };
    validate_post_update_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, &post.visibility)?;
    update_post(db_client, &post.id, post_data).await?;
    Ok(Some(NOTE))
}

#[derive(Deserialize)]
struct UpdatePerson {
    actor: String,
    object: Actor,
}

async fn handle_update_person(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let activity: UpdatePerson = serde_json::from_value(activity)
        .map_err(|_| ValidationError("invalid actor data"))?;
    if activity.object.id != activity.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };
    let profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.object.id,
    ).await?;
    update_remote_profile(
        db_client,
        &config.instance(),
        &MediaStorage::from(config),
        profile,
        activity.object,
    ).await?;
    Ok(Some(PERSON))
}

pub async fn handle_update(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    mut activity: JsonValue,
    is_authenticated: bool,
) -> HandlerResult {
    let is_not_embedded = activity["object"].as_str().is_some();
    if is_not_embedded || !is_authenticated {
        // Fetch object if it is not embedded or if activity is forwarded
        let object_id = find_object_id(&activity["object"])?;
        activity["object"] = fetch_object(
            &config.instance(),
            &object_id,
        ).await?;
        log::info!("fetched object {}", object_id);
    };
    let object_type = activity["object"]["type"].as_str()
        .ok_or(ValidationError("unknown object type"))?;
    match object_type {
        ARTICLE | NOTE => {
            handle_update_note(config, db_client, activity).await
        },
        GROUP | PERSON => {
            handle_update_person(config, db_client, activity).await
        },
        _ => {
            log::warn!("unexpected object type {}", object_type);
            Ok(None)
        },
    }
}
