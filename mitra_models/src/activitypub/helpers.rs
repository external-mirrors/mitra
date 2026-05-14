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
    maybe_after_object_id: Option<&str>,
    limit: u16,
) -> Result<Vec<JsonValue>, DatabaseError> {
    let items = get_collection_items(
        db_client,
        collection_id,
        maybe_after_object_id,
        limit,
    )
        .await?
        .into_iter()
        .map(|object| object.object_data)
        .collect();
    Ok(items)
}
