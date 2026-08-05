use uuid::Uuid;

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
    notifications::helpers::create_moderation_warning_notification,
};

use super::{
    queries::create_moderation_action,
    types::ModerationActionType,
};

pub async fn on_local_post_deleted(
    db_client: &impl DatabaseClient,
    moderator_id: Uuid,
    author_id: Uuid,
) -> Result<(), DatabaseError> {
    // Author must be a local actor
    let action = create_moderation_action(
        db_client,
        moderator_id,
        author_id,
        ModerationActionType::PostDeleted,
        None, // no reason
    ).await?;
    create_moderation_warning_notification(
        db_client,
        moderator_id,
        author_id,
        action.id,
    ).await?;
    Ok(())
}
