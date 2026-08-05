use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
};
use super::types::{ModerationAction, ModerationActionType};

pub(super) async fn create_moderation_action(
    db_client: &impl DatabaseClient,
    moderator_id: Uuid,
    target_id: Uuid,
    action_type: ModerationActionType,
    reason: Option<String>,
) -> Result<ModerationAction, DatabaseError> {
    let action_id = generate_ulid();
    let row = db_client.query_one(
        "
        INSERT INTO moderation_action (
            id,
            moderator_id,
            target_id,
            action_type,
            reason
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING moderation_action
        ",
        &[
            &action_id,
            &moderator_id,
            &target_id,
            &action_type,
            &reason,
        ],
    ).await?;
    let action = row.try_get("moderation_action")?;
    Ok(action)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        accounts::test_utils::create_test_user,
        database::test_utils::create_test_database,
        posts::test_utils::create_test_local_post,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_moderation_action() {
        let db_client = &mut create_test_database().await;
        let moderator = create_test_user(db_client, "moderator").await;
        let author = create_test_user(db_client, "author").await;
        let _post = create_test_local_post(
            db_client,
            author.id,
            "test post",
        ).await;
        let action = create_moderation_action(
            db_client,
            moderator.id,
            author.id,
            ModerationActionType::PostDeleted,
            Some("post 123".to_owned()),
        ).await.unwrap();
        assert_eq!(action.moderator_id, moderator.id);
        assert_eq!(action.target_id, author.id);
        assert_eq!(action.action_type, ModerationActionType::PostDeleted);
        assert_eq!(action.reason.unwrap(), "post 123");
    }
}
