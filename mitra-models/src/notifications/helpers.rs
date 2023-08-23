use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};

use super::queries::create_notification;
use super::types::EventType;

pub async fn create_follow_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::Follow,
    ).await
}

pub async fn create_reply_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Reply,
    ).await
}

pub async fn create_reaction_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Reaction,
    ).await
}

pub async fn create_mention_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Mention,
    ).await
}

pub async fn create_repost_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Repost,
    ).await
}

pub async fn create_subscription_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::Subscription,
    ).await
}

pub async fn create_subscription_expiration_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::SubscriptionExpiration,
    ).await
}

pub async fn create_move_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::Move,
    ).await
}
