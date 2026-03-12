use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    relationships::queries::unfollow,
};

use crate::{
    authority::Authority,
    identifiers::canonicalize_id,
    importers::{
        get_user_by_actor_id,
        ActorIdResolver,
        ApClient,
    },
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
struct Block {
    actor: String,
    object: String,
}

pub async fn handle_block(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let block: Block = serde_json::from_value(activity)?;
    let source_profile = ActorIdResolver::default().only_remote().resolve(
        ap_client,
        db_pool,
        &block.actor,
    ).await?;
    let canonical_object_id = canonicalize_id(&block.object)?;
    let db_client = &mut **get_database_client(db_pool).await?;
    let authority = Authority::from(&ap_client.instance);
    let target_user = get_user_by_actor_id(
        db_client,
        &authority,
        &canonical_object_id,
    ).await?;
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
