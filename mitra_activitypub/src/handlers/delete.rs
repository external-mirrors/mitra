use serde::Deserialize;
use serde_json::Value;

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

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Delete {
    #[serde(deserialize_with = "deserialize_into_object_id")]
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_delete(
    _config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: Delete = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    if activity.object == activity.actor {
        // Self-delete
        let profile = match get_remote_profile_by_actor_id(
            db_client,
            &activity.object,
        ).await {
            Ok(profile) => profile,
            // Ignore Delete(Person) if profile is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        deletion_queue.into_job(db_client).await?;
        log::info!("deleted remote actor {}", activity.object);
        return Ok(Some(Descriptor::object("Actor")));
    };
    let post = match get_remote_post_by_object_id(
        db_client,
        &activity.object,
    ).await {
        Ok(post) => post,
        // Ignore Delete(Note) if post is not found
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let actor_profile = get_remote_profile_by_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    if post.author.id != actor_profile.id {
        return Err(ValidationError("actor is not an author").into());
    };
    let deletion_queue = delete_post(db_client, post.id).await?;
    deletion_queue.into_job(db_client).await?;
    Ok(Some(Descriptor::object("Object")))
}
