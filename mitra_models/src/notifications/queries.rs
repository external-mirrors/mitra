use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};
use crate::posts::{
    helpers::{add_related_posts, add_user_actions},
    queries::post_subqueries,
};
use crate::relationships::types::RelationshipType;

use super::types::{EventType, NotificationDetailed};

pub(super) async fn create_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Option<Uuid>,
    reaction_id: Option<Uuid>,
    event_type: EventType,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO notification (
            sender_id,
            recipient_id,
            post_id,
            reaction_id,
            event_type
        )
        VALUES ($1, $2, $3, $4, $5)
        ",
        &[
            &sender_id,
            &recipient_id,
            &post_id,
            &reaction_id,
            &event_type,
        ],
    ).await?;
    Ok(())
}

pub async fn get_notifications(
    db_client: &impl DatabaseClient,
    recipient_id: Uuid,
    min_id: Option<i32>,
    max_id: Option<i32>,
    limit: u16,
) -> Result<Vec<NotificationDetailed>, DatabaseError> {
    let is_forward_paginated = min_id.is_some();
    let statement = format!(
        "
        SELECT
            notification,
            sender,
            post,
            post_author,
            {post_subqueries},
            post_reaction.content AS reaction_content,
            emoji AS reaction_emoji
        FROM notification
        JOIN actor_profile AS sender
        ON notification.sender_id = sender.id
        LEFT JOIN post
        ON notification.post_id = post.id
        LEFT JOIN actor_profile AS post_author
        ON post.author_id = post_author.id
        LEFT JOIN post_reaction
        ON notification.reaction_id = post_reaction.id
        LEFT JOIN emoji
        ON post_reaction.emoji_id = emoji.id
        WHERE
            recipient_id = $1
            AND NOT EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = notification.recipient_id
                    AND target_id = notification.sender_id
                    AND relationship_type = {relationship_mute}
            )
            AND ($2::integer IS NULL OR notification.id > $2)
            AND ($3::integer IS NULL OR notification.id < $3)
        ORDER BY notification.id {order}
        LIMIT $4
        ",
        post_subqueries=post_subqueries(),
        relationship_mute=i16::from(RelationshipType::Mute),
        order=if is_forward_paginated { "ASC" } else { "DESC" },
    );
    let rows = db_client.query(
        &statement,
        &[
            &recipient_id,
            &min_id,
            &max_id,
            &i64::from(limit),
        ],
    ).await?;
    let mut notifications: Vec<_> = rows.iter()
        .map(NotificationDetailed::try_from)
        .collect::<Result<_, _>>()?;
    if is_forward_paginated {
        notifications.reverse();
    };
    add_related_posts(
        db_client,
        notifications.iter_mut()
            .filter_map(|item| item.post.as_mut())
            .collect(),
    ).await?;
    add_user_actions(
        db_client,
        recipient_id,
        notifications.iter_mut()
            .filter_map(|item| item.post.as_mut())
            .collect(),
    ).await?;
    Ok(notifications)
}

pub async fn delete_notifications(
    db_client: &impl DatabaseClient,
    recipient_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        DELETE FROM notification
        WHERE recipient_id = $1
        ",
        &[&recipient_id],
    ).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        posts::test_utils::create_test_local_post,
        reactions::test_utils::create_test_local_reaction,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_get_notifications() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let post = create_test_local_post(db_client, user_1.id, "test").await;
        create_test_local_reaction(
            db_client,
            user_2.id,
            post.id,
            Some("❤️"),
        ).await;
        let notifications = get_notifications(
            db_client,
            user_1.id,
            None,
            None,
            5,
        ).await.unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].sender.id, user_2.id);
        assert_eq!(notifications[0].post.as_ref().unwrap().id, post.id);
        assert_eq!(notifications[0].event_type, EventType::Reaction);
        assert_eq!(notifications[0].reaction_content, Some("❤️".to_string()));
        assert_eq!(notifications[0].reaction_emoji.is_none(), true);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_notifications_pagination() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let post = create_test_local_post(db_client, user_1.id, "test").await;
        create_test_local_reaction(db_client, user_2.id, post.id, Some("a")).await;
        create_test_local_reaction(db_client, user_2.id, post.id, Some("b")).await;
        create_test_local_reaction(db_client, user_2.id, post.id, Some("c")).await;
        create_test_local_reaction(db_client, user_2.id, post.id, Some("d")).await;
        create_test_local_reaction(db_client, user_2.id, post.id, Some("e")).await;
        let notifications = get_notifications(
            db_client,
            user_1.id,
            None,
            None,
            5,
        ).await.unwrap();
        assert_eq!(notifications.len(), 5);

        let notifications_backward = get_notifications(
            db_client,
            user_1.id,
            None,
            Some(notifications[0].id),
            2,
        ).await.unwrap();
        assert_eq!(notifications_backward.len(), 2);
        assert_eq!(notifications_backward[0].id, notifications[1].id);
        assert_eq!(notifications_backward[1].id, notifications[2].id);

        let notifications_forward = get_notifications(
            db_client,
            user_1.id,
            Some(notifications[4].id),
            None,
            2,
        ).await.unwrap();
        assert_eq!(notifications_forward.len(), 2);
        assert_eq!(notifications_forward[0].id, notifications[2].id);
        assert_eq!(notifications_forward[1].id, notifications[3].id);
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_notifications() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let post = create_test_local_post(db_client, user_1.id, "test").await;
        create_test_local_reaction(db_client, user_2.id, post.id, None).await;
        let notifications = get_notifications(
            db_client,
            user_1.id,
            None,
            None,
            5,
        ).await.unwrap();
        assert_eq!(notifications.len(), 1);
        delete_notifications(db_client, user_1.id).await.unwrap();
        let notifications = get_notifications(
            db_client,
            user_1.id,
            None,
            None,
            5,
        ).await.unwrap();
        assert_eq!(notifications.len(), 0);
    }
}
