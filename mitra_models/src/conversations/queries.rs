use chrono::{TimeZone, Utc};
use uuid::Uuid;

use mitra_utils::id::{generate_deterministic_ulid, generate_ulid};

use crate::{
    database::{
        catch_unique_violation,
        DatabaseClient,
        DatabaseError,
    },
    posts::{
        queries::post_subqueries,
        types::Visibility,
    },
    relationships::types::RelationshipType,
};

use super::types::{
    Conversation,
    ConversationPreview,
    TrackingStatus,
};

pub async fn create_conversation(
    db_client: &impl DatabaseClient,
    root_id: Uuid,
    is_managed: bool,
    object_id: Option<&str>,
    audience: Option<&str>,
) -> Result<Conversation, DatabaseError> {
    let conversation_id = generate_ulid();
    let row = db_client.query_one(
        "
        INSERT INTO conversation (
            id,
            root_id,
            is_managed,
            object_id,
            audience
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING conversation
        ",
        &[
            &conversation_id,
            &root_id,
            &is_managed,
            &object_id,
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

pub async fn get_direct_conversations(
    db_client: &impl DatabaseClient,
    account_id: Uuid,
    max_conversation_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<ConversationPreview>, DatabaseError> {
    let uuid4_min_date = Utc
        .with_ymd_and_hms(2030, 1, 1, 0, 0, 0)
        // Date should be valid
        .unwrap();
    let uuid4_min_id = generate_deterministic_ulid("", uuid4_min_date);
    let statement = format!(
        "
        SELECT
            ARRAY(
                SELECT actor_profile
                FROM post_mention
                JOIN actor_profile ON post_mention.profile_id = actor_profile.id
                WHERE post_id = root.id
                UNION
                SELECT actor_profile
                FROM actor_profile
                WHERE actor_profile.id = root.author_id
            ) AS participants,
            post,
            post_author,
            {post_subqueries}
        FROM conversation
        JOIN post AS root
            ON conversation.root_id = root.id
        CROSS JOIN LATERAL (
            SELECT post.id
            FROM post
            WHERE
                post.conversation_id = conversation.id
                AND post.visibility = {visibility_direct}
                AND (
                    post.author_id = $1
                    OR EXISTS (
                        SELECT 1 FROM post_mention
                        WHERE post_id = post.id AND profile_id = $1
                    )
                )
            ORDER BY post.created_at DESC
            LIMIT 1
        ) AS last_dm
        JOIN post
            ON last_dm.id = post.id
        JOIN actor_profile AS post_author
            ON post.author_id = post_author.id
        WHERE
            -- audience filter is redundant but speeds up the query
            conversation.audience IS NULL
            AND root.visibility = {visibility_direct}
            AND (
                root.author_id = $1
                OR EXISTS (
                    SELECT 1 FROM post_mention
                    WHERE post_id = root.id AND profile_id = $1
                )
            )
            -- exclude v4 UUIDs produced in migration
            AND conversation.id < '{uuid4_min_id}'::uuid
            AND ($2::uuid IS NULL OR conversation.id < $2)
        ORDER BY conversation.id DESC
        LIMIT $3
        ",
        post_subqueries=post_subqueries(),
        visibility_direct=i16::from(Visibility::Direct),
        uuid4_min_id=uuid4_min_id,
    );
    let rows = db_client.query(
        &statement,
        &[
            &account_id,
            &max_conversation_id,
            &i64::from(limit),
        ],
    ).await?;
    let previews = rows.iter()
        .map(ConversationPreview::try_from)
        .collect::<Result<_, _>>()?;
    Ok(previews)
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
        posts::{
            queries::create_post,
            test_utils::create_test_local_post,
            types::{PostContext, PostCreateData},
        },
        profiles::test_utils::create_test_local_profile,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_get_direct_conversations() {
        let db_client = &mut create_test_database().await;
        let sender = create_test_user(db_client, "sender").await;
        let recipient = create_test_user(db_client, "recipient").await;
        let post_data_1 = PostCreateData {
            context: PostContext::Top { object_id: None, audience: None },
            content: "test".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![recipient.id],
            ..PostCreateData::for_test()
        };
        let post_1 = create_post(db_client, sender.id, post_data_1).await.unwrap();
        let post_data_2 = PostCreateData {
            context: PostContext::reply_to(&post_1),
            content: "reply".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![sender.id],
            ..PostCreateData::for_test()
        };
        let post_2 = create_post(db_client, recipient.id, post_data_2).await.unwrap();
        let conversations = get_direct_conversations(
            db_client,
            recipient.id,
            None,
            20,
        ).await.unwrap();
        assert_eq!(conversations.len(), 1);
        let conversation_preview = &conversations[0];
        assert_eq!(conversation_preview.conversation.root_id, post_1.id);
        assert_eq!(conversation_preview.participants.len(), 2);
        assert_eq!(conversation_preview.last_post.id, post_2.id);
    }

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
