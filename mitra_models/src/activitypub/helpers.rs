use serde_json::{Value as JsonValue};

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
};
use super::queries::get_collection_items;

pub async fn get_collection_items_json(
    db_client: &impl DatabaseClient,
    collection_id: &str,
    limit: u16,
) -> Result<Vec<JsonValue>, DatabaseError> {
    let items = get_collection_items(db_client, collection_id, limit)
        .await?
        .into_iter()
        .map(|object| object.object_data)
        .collect();
    Ok(items)
}

pub async fn get_object_ids(
    db_client: &impl DatabaseClient,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile.actor_id AS object_id
        FROM actor_profile
        WHERE actor_profile.actor_json IS NOT NULL
        UNION ALL
        SELECT post.object_id
        FROM post
        WHERE post.object_id IS NOT NULL
        ",
        &[],
    ).await?;
    let object_ids = rows.iter()
        .map(|row| row.try_get("object_id"))
        .collect::<Result<_, _>>()?;
    Ok(object_ids)
}
