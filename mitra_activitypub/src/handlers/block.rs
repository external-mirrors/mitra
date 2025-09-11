use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    relationships::queries::unfollow,
    users::queries::get_user_by_name,
};

use crate::{
    identifiers::parse_local_actor_id,
    importers::{ActorIdResolver, ApClient},
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Block {
    actor: String,
    object: String,
}

pub async fn handle_block(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let block: Block = serde_json::from_value(activity)?;
    let db_client = &mut **get_database_client(db_pool).await?;
    let ap_client = ApClient::new(config, db_client).await?;
    let source_profile = ActorIdResolver::default().only_remote().resolve(
        &ap_client,
        db_client,
        &block.actor,
    ).await?;
    let target_username = parse_local_actor_id(
        &config.instance_url(),
        &block.object,
    )?;
    let target_user = get_user_by_name(db_client, &target_username).await?;
    // Similar to Undo(Follow)
    match unfollow(db_client, source_profile.id, target_user.id).await {
        Ok(_) | Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    // Similar to Reject(Follow)
    match unfollow(db_client, target_user.id, source_profile.id).await {
        Ok(_) | Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(Descriptor::object("Actor")))
}
