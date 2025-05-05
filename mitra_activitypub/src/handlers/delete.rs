use serde::Deserialize;
use serde_json::{Value as JsonValue};

use apx_sdk::deserialization::deserialize_into_object_id;
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::{
        delete_post,
        get_remote_post_by_object_id,
    },
    profiles::queries::{
        delete_profile,
        get_remote_profile_by_actor_id,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    builders::add_context_activity::sync_conversation,
    importers::ApClient,
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Delete {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_delete(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: JsonValue,
) -> HandlerResult {
    let delete: Delete = serde_json::from_value(activity.clone())?;
    let ap_client = ApClient::new(config, db_client).await?;
    if delete.object == delete.actor {
        // Self-delete
        let profile = match get_remote_profile_by_actor_id(
            db_client,
            &delete.object,
        ).await {
            Ok(profile) => profile,
            // Ignore Delete(Person) if profile is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        let deletion_queue = delete_profile(db_client, profile.id).await?;
        deletion_queue.into_job(db_client).await?;
        log::info!("deleted remote actor {}", delete.object);
        return Ok(Some(Descriptor::object("Actor")));
    };
    // Delete(Note)
    let post = match get_remote_post_by_object_id(
        db_client,
        &delete.object,
    ).await {
        Ok(post) => post,
        // Ignore Delete(Note) if post is not found
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &delete.actor,
    ).await?;
    if post.author.id != actor_profile.id {
        return Err(ValidationError("actor is not an author").into());
    };
    let deletion_queue = delete_post(db_client, post.id).await?;
    deletion_queue.into_job(db_client).await?;
    sync_conversation(
        db_client,
        &ap_client.instance,
        post.expect_conversation(),
        activity,
        post.visibility,
    ).await?;
    Ok(Some(Descriptor::object("Object")))
}
