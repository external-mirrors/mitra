use uuid::Uuid;

use crate::database::{
    DatabaseClient,
    DatabaseError,
};

use super::queries::{create_follow_request_unchecked, has_relationship};
use super::types::{DbFollowRequest, RelationshipType};

pub async fn create_follow_request(
    db_client: &mut impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Prevent changes to relationship table
    transaction.execute(
        "LOCK TABLE relationship IN EXCLUSIVE MODE",
        &[],
    ).await?;
    let is_following = has_relationship(
        &transaction,
        source_id,
        target_id,
        RelationshipType::Follow,
    ).await?;
    if is_following {
        // Follow request should not be created if
        // follow relationship exists.
        return Err(DatabaseError::AlreadyExists("relationship"));
    };
    let request = create_follow_request_unchecked(
        &transaction,
        source_id,
        target_id,
    ).await?;
    transaction.commit().await?;
    Ok(request)
}
