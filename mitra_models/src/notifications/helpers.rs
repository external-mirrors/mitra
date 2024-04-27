use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};
use crate::relationships::{
    queries::has_relationship,
    types::RelationshipType,
};
use crate::users::{
    queries::get_users_by_role,
    types::Role,
};

use super::queries::create_notification;
use super::types::EventType;

pub async fn create_follow_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<(), DatabaseError> {
    if has_relationship(
        db_client,
        recipient_id,
        sender_id,
        RelationshipType::Mute
    ).await? {
        return Ok(());
    };
    create_notification(
        db_client, sender_id, recipient_id, None,
        None,
        EventType::Follow,
    ).await
}

pub async fn create_follow_request_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<(), DatabaseError> {
    if has_relationship(
        db_client,
        recipient_id,
        sender_id,
        RelationshipType::Mute
    ).await? {
        return Ok(());
    };
    create_notification(
        db_client, sender_id, recipient_id, None,
        None,
        EventType::FollowRequest,
    ).await
}

pub async fn create_reply_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Uuid,
) -> Result<(), DatabaseError> {
    if has_relationship(
        db_client,
        recipient_id,
        sender_id,
        RelationshipType::Mute
    ).await? {
        return Ok(());
    };
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        None,
        EventType::Reply,
    ).await
}

pub async fn create_reaction_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Uuid,
    reaction_id: Uuid,
) -> Result<(), DatabaseError> {
    if has_relationship(
        db_client,
        recipient_id,
        sender_id,
        RelationshipType::Mute
    ).await? {
        return Ok(());
    };
    create_notification(
        db_client, sender_id, recipient_id,
        Some(post_id),
        Some(reaction_id),
        EventType::Reaction,
    ).await
}

pub async fn create_mention_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Uuid,
) -> Result<(), DatabaseError> {
    if has_relationship(
        db_client,
        recipient_id,
        sender_id,
        RelationshipType::Mute
    ).await? {
        return Ok(());
    };
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        None,
        EventType::Mention,
    ).await
}

pub async fn create_repost_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Uuid,
) -> Result<(), DatabaseError> {
    if has_relationship(
        db_client,
        recipient_id,
        sender_id,
        RelationshipType::Mute
    ).await? {
        return Ok(());
    };
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        None,
        EventType::Repost,
    ).await
}

pub async fn create_subscriber_payment_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        None,
        EventType::SubscriberPayment,
    ).await
}

pub async fn create_subscriber_leaving_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        None,
        EventType::SubscriberLeaving,
    ).await
}

pub async fn create_subscription_expiration_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        None,
        EventType::SubscriptionExpiration,
    ).await
}

pub async fn create_move_notification(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        None,
        EventType::Move,
    ).await
}

pub async fn create_signup_notifications(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
) -> Result<(), DatabaseError> {
    let admins = get_users_by_role(db_client, Role::Admin).await?;
    for recipient_id in admins {
        create_notification(
            db_client, sender_id, recipient_id, None,
            None,
            EventType::SignUp,
        ).await?;
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::notifications::queries::get_notifications;
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_follow_notification() {
        let db_client = &mut create_test_database().await;
        let user_data_1 = UserCreateData {
            username: "test1".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user_1 = create_user(db_client, user_data_1).await.unwrap();
        let user_data_2 = UserCreateData {
            username: "test2".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user_2 = create_user(db_client, user_data_2).await.unwrap();
        create_follow_notification(
            db_client,
            user_2.id,
            user_1.id,
        ).await.unwrap();
        let notifications = get_notifications(
            db_client,
            user_1.id,
            None,
            5,
        ).await.unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].sender.id, user_2.id);
        assert_eq!(notifications[0].post.is_none(), true);
        assert_eq!(notifications[0].event_type, EventType::Follow);
    }
}
