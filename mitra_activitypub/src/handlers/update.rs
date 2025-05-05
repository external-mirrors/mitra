use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    authentication::{
        verify_portable_object,
        AuthenticationError,
    },
    deserialization::{deserialize_into_object_id, object_to_id},
    utils::{is_actor, is_object},
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::get_remote_post_by_object_id,
    profiles::queries::get_remote_profile_by_actor_id,
};
use mitra_validators::errors::ValidationError;

use crate::{
    actors::handlers::{update_remote_profile, Actor},
    builders::add_context_activity::sync_conversation,
    identifiers::canonicalize_id,
    importers::ApClient,
    ownership::verify_object_owner,
};

use super::{
    note::{
        get_object_attributed_to,
        update_remote_post,
        AttributedObjectJson,
    },
    Descriptor,
    HandlerResult,
};

#[derive(Deserialize)]
struct UpdateNote {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    object: AttributedObjectJson,
}

async fn handle_update_note(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let update: UpdateNote = serde_json::from_value(activity.clone())?;
    let object = update.object;
    let canonical_object_id = canonicalize_id(object.id())?;
    let author_id = get_object_attributed_to(&object.inner)?;
    if author_id != update.actor {
        return Err(ValidationError("attributedTo value doesn't match actor").into());
    };
    let post = match get_remote_post_by_object_id(
        db_client,
        &canonical_object_id.to_string(),
    ).await {
        Ok(post) => post,
        // Ignore Update if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let ap_client = ApClient::new(config, db_client).await?;
    let post = update_remote_post(
        &ap_client,
        db_client,
        post,
        &object,
    ).await?;
    sync_conversation(
        db_client,
        &ap_client.instance,
        post.expect_conversation(),
        activity,
        post.visibility,
    ).await?;
    Ok(Some(Descriptor::object(object.inner.object_type)))
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
    let update: UpdatePerson = serde_json::from_value(activity)?;
    if update.object.id() != update.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };
    let canonical_actor_id = canonicalize_id(update.object.id())?;
    let profile = match get_remote_profile_by_actor_id(
        db_client,
        &canonical_actor_id.to_string(),
    ).await {
        Ok(profile) => profile,
        // Ignore Update if profile is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let ap_client = ApClient::new(config, db_client).await?;
    let profile = update_remote_profile(
        &ap_client,
        db_client,
        profile,
        update.object,
    ).await?;
    let actor_type = &profile.expect_actor_data().object_type;
    Ok(Some(Descriptor::object(actor_type)))
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
        let object_id = object_to_id(&activity["object"])
            .map_err(|_| ValidationError("invalid activity object"))?;
        let ap_client = ApClient::new(config, db_client).await?;
        activity["object"] = ap_client.fetch_object(&object_id).await?;
        log::info!("fetched object {}", object_id);
    };
    match verify_portable_object(&activity["object"]) {
        Ok(_) => (),
        Err(AuthenticationError::InvalidObjectID(message)) => {
            return Err(ValidationError(message).into());
        },
        Err(AuthenticationError::NotPortable) => (),
        Err(_) => {
            return Err(ValidationError("invalid portable object").into());
        },
    };
    if is_actor(&activity["object"]) {
        handle_update_person(config, db_client, activity).await
    } else if is_object(&activity["object"]) {
        verify_object_owner(&activity["object"])?;
        handle_update_note(config, db_client, activity).await
    } else {
        log::warn!("unexpected object structure: {}", activity["object"]);
        Ok(None)
    }
}
