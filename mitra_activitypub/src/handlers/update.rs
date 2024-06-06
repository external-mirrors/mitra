use std::collections::HashMap;

use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_adapters::permissions::filter_mentions;
use mitra_config::Config;
use mitra_federation::{
    deserialization::{deserialize_into_object_id, get_object_id},
    utils::{is_actor, is_object},
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::{
        get_post_by_remote_object_id,
        update_post,
    },
    posts::types::{PostUpdateData, Visibility},
    profiles::queries::get_profile_by_remote_actor_id,
};
use mitra_services::media::MediaStorage;
use mitra_validators::{
    errors::ValidationError,
    posts::{
        validate_post_mentions,
        validate_post_update_data,
    },
};

use crate::{
    actors::handlers::{update_remote_profile, Actor},
    agent::build_federation_agent,
    handlers::create::{
        create_content_link,
        get_object_attachments,
        get_object_attributed_to,
        get_object_content,
        get_object_tags,
        get_object_url,
        AttributedObject,
    },
    identifiers::profile_actor_id,
    importers::fetch_any_object,
    vocabulary::{NOTE, PERSON},
};

use super::HandlerResult;

#[derive(Deserialize)]
struct UpdateNote {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: AttributedObject,
}

async fn handle_update_note(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let activity: UpdateNote = serde_json::from_value(activity)
        .map_err(|_| ValidationError("invalid object"))?;
    let object = activity.object;
    let author_id = get_object_attributed_to(&object)?;
    if author_id != activity.actor {
        return Err(ValidationError("attributedTo value doesn't match actor").into());
    };
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
    if profile_actor_id(&instance.url(), &post.author) != author_id {
        return Err(ValidationError("object owner can't be changed").into());
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
    validate_post_mentions(&post_data.mentions, &post.visibility)?;
    let (_, deletion_queue) =
        update_post(db_client, &post.id, post_data).await?;
    deletion_queue.into_job(db_client).await?;
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
    let profile = match get_profile_by_remote_actor_id(
        db_client,
        &activity.object.id,
    ).await {
        Ok(profile) => profile,
        // Ignore Update if profile is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let agent = build_federation_agent(&config.instance(), None);
    update_remote_profile(
        &agent,
        db_client,
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
        let object_id = get_object_id(&activity["object"])
            .map_err(|_| ValidationError("invalid activity object"))?;
        let agent = build_federation_agent(&config.instance(), None);
        activity["object"] = fetch_any_object(&agent, &object_id).await?;
        log::info!("fetched object {}", object_id);
    };
    if is_actor(&activity["object"]) {
        handle_update_person(config, db_client, activity).await
    } else if is_object(&activity["object"]) {
        handle_update_note(config, db_client, activity).await
    } else {
        log::warn!("unexpected object structure: {}", activity["object"]);
        Ok(None)
    }
}
