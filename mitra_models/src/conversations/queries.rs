use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::{
    database::{
        catch_unique_violation,
        DatabaseClient,
        DatabaseError,
    },
    posts::types::Visibility,
    relationships::types::RelationshipType,
};

use super::types::{Conversation, TrackingStatus};

pub async fn create_conversation(
    db_client: &impl DatabaseClient,
    root_id: Uuid,
    audience: Option<&str>,
) -> Result<Conversation, DatabaseError> {
    let conversation_id = generate_ulid();
    let row = db_client.query_one(
        "
        INSERT INTO conversation (
            id,
            root_id,
            audience
        )
        VALUES ($1, $2, $3)
        RETURNING conversation
        ",
        &[
            &conversation_id,
            &root_id,
            &audience,
        ],
    ).await.map_err(catch_unique_violation("conversation"))?;
    let conversation = row.try_get("conversation")?;
    Ok(conversation)
}

pub async fn get_conversation(
    db_client: &impl DatabaseClient,
    conversation_id: Uuid,
) -> Result<Conversation, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT conversation FROM conversation
        WHERE conversation.id = $1
        ",
        &[&conversation_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("conversation"))?;
    let conversation = row.try_get("conversation")?;
    Ok(conversation)
}

pub async fn is_conversation_participant(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<bool, DatabaseError> {
    let statement = format!(
        "
        SELECT 1
        FROM conversation
        JOIN post AS root ON conversation.root_id = root.id
        WHERE
            conversation.id = $2
            AND (
                root.author_id = $1
                OR EXISTS (
                    SELECT 1 FROM relationship
                    WHERE
                        relationship.source_id = $1
                        AND relationship.target_id = root.author_id
                        AND (
                            root.visibility = {visibility_followers}
                            AND relationship_type = {relationship_follow}
                            OR root.visibility = {visibility_subscribers}
                            AND relationship_type = {relationship_subscription}
                        )
                )
            )
        ",
        visibility_followers=i16::from(Visibility::Followers),
        visibility_subscribers=i16::from(Visibility::Subscribers),
        relationship_follow=i16::from(RelationshipType::Follow),
        relationship_subscription=i16::from(RelationshipType::Subscription),
    );
    let maybe_row = db_client.query_opt(
        &statement,
        &[&user_id, &conversation_id],
    ).await?;
    Ok(maybe_row.is_some())
}

pub async fn set_conversation_tracking_status(
    db_client: &impl DatabaseClient,
    conversation_id: Uuid,
    account_id: Uuid,
    maybe_tracking_status: Option<TrackingStatus>,
) -> Result<(), DatabaseError> {
    if let Some(tracking_status) = maybe_tracking_status {
        db_client.execute(
            "
            INSERT INTO conversation_tracking (
                conversation_id,
                account_id,
                tracking_status
            )
            VALUES ($1, $2, $3)
            ON CONFLICT (conversation_id, account_id)
            DO UPDATE SET tracking_status = $3
            ",
            &[
                &conversation_id,
                &account_id,
                &tracking_status,
            ],
        ).await?;
    } else {
        db_client.execute(
            "
            DELETE FROM conversation_tracking
            WHERE conversation_id = $1 AND account_id = $2
            ",
            &[
                &conversation_id,
                &account_id,
            ],
        ).await?;
    };
    Ok(())
}

/// Finds conversation tracking statuses for given posts
pub(crate) async fn find_tracking_statuses_by_user(
    db_client: &impl DatabaseClient,
    account_id: Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<(Uuid, Option<TrackingStatus>)>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post.id, tracking_status
        FROM post
        LEFT JOIN conversation_tracking
        ON
            conversation_tracking.conversation_id = post.conversation_id
            AND conversation_tracking.account_id = $1
        WHERE post.id = ANY($2)
        ",
        &[&account_id, &posts_ids],
    ).await?;
    let statuses = rows.iter()
        .map(|row| {
            let post_id = row.try_get("id")?;
            let tracking_status = row.try_get("tracking_status")?;
            Ok((post_id, tracking_status))
        })
        .collect::<Result<Vec<_>, DatabaseError>>()?;
    Ok(statuses)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        posts::test_utils::create_test_local_post,
        profiles::test_utils::create_test_local_profile,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_tracking_status() {
        let db_client = &mut create_test_database().await;
        let author = create_test_local_profile(db_client, "author").await;
        let viewer = create_test_local_profile(db_client, "viewer").await;
        let post = create_test_local_post(db_client, author.id, "test").await;
        let conversation_id = post.expect_conversation().id;
        // Set status
        set_conversation_tracking_status(
            db_client,
            conversation_id,
            viewer.id,
            Some(TrackingStatus::Follow),
        ).await.unwrap();
        let statuses = find_tracking_statuses_by_user(
            db_client,
            viewer.id,
            &[post.id],
        ).await.unwrap();
        assert_eq!(statuses[0].1, Some(TrackingStatus::Follow));
        // Remove status
        set_conversation_tracking_status(
            db_client,
            conversation_id,
            viewer.id,
            None,
        ).await.unwrap();
        let statuses = find_tracking_statuses_by_user(
            db_client,
            viewer.id,
            &[post.id],
        ).await.unwrap();
        assert_eq!(statuses[0].1, None);
    }
}
