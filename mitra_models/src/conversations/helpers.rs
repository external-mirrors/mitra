use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};
use super::{
    queries::find_tracking_statuses_by_user,
    types::TrackingStatus,
};

pub async fn is_conversation_muted(
    db_client: &impl DatabaseClient,
    account_id: Uuid,
    post_id: Uuid,
) -> Result<bool, DatabaseError> {
    let statuses = find_tracking_statuses_by_user(
        db_client,
        account_id,
        &[post_id],
    ).await?;
    let is_muted = statuses.into_iter().any(|(id, maybe_status)| {
        id == post_id && maybe_status == Some(TrackingStatus::Mute)
    });
    Ok(is_muted)
}
