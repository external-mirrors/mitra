use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};
use crate::relationships::{
    queries::subscribe_opt,
    types::RelationshipType,
};

use super::types::{DbSubscription, Subscription};

pub async fn create_subscription(
    db_client: &mut impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
    expires_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> Result<DbSubscription, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let row = transaction.query_one(
        "
        INSERT INTO subscription (
            sender_id,
            recipient_id,
            expires_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4)
        RETURNING subscription
        ",
        &[
            &sender_id,
            &recipient_id,
            &expires_at,
            &updated_at,
        ],
    ).await.map_err(catch_unique_violation("subscription"))?;
    let subscription: DbSubscription = row.try_get("subscription")?;
    subscribe_opt(
        &mut transaction,
        subscription.sender_id,
        subscription.recipient_id,
    ).await?;
    transaction.commit().await?;
    Ok(subscription)
}

pub async fn update_subscription(
    db_client: &mut impl DatabaseClient,
    subscription_id: i32,
    expires_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> Result<DbSubscription, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        UPDATE subscription
        SET
            expires_at = $2,
            updated_at = $3
        WHERE id = $1
        RETURNING subscription
        ",
        &[
            &subscription_id,
            &expires_at,
            &updated_at,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("subscription"))?;
    let subscription: DbSubscription = row.try_get("subscription")?;
    if expires_at > Utc::now() {
        subscribe_opt(
            &mut transaction,
            subscription.sender_id,
            subscription.recipient_id,
        ).await?;
    };
    transaction.commit().await?;
    Ok(subscription)
}

pub async fn get_subscription_by_participants(
    db_client: &impl DatabaseClient,
    sender_id: Uuid,
    recipient_id: Uuid,
) -> Result<DbSubscription, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT subscription
        FROM subscription
        WHERE sender_id = $1 AND recipient_id = $2
        ",
        &[&sender_id, &recipient_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("subscription"))?;
    let subscription: DbSubscription = row.try_get("subscription")?;
    Ok(subscription)
}

pub async fn get_expired_subscriptions(
    db_client: &impl DatabaseClient,
) -> Result<Vec<DbSubscription>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT subscription
        FROM subscription
        JOIN relationship
        ON (
            relationship.source_id = subscription.sender_id
            AND relationship.target_id = subscription.recipient_id
            AND relationship.relationship_type = $1
        )
        WHERE subscription.expires_at <= CURRENT_TIMESTAMP
        ",
        &[&RelationshipType::Subscription],
    ).await?;
   let subscriptions = rows.iter()
        .map(|row| row.try_get("subscription"))
        .collect::<Result<_, _>>()?;
    Ok(subscriptions)
}

pub async fn get_incoming_subscriptions(
    db_client: &impl DatabaseClient,
    recipient_id: Uuid,
    include_expired: bool,
    max_subscription_id: Option<i32>,
    limit: u16,
) -> Result<Vec<Subscription>, DatabaseError> {
    let mut filter = "subscription.recipient_id = $1".to_owned();
    if !include_expired {
        filter += " AND subscription.expires_at > CURRENT_TIMESTAMP";
    };
    let statement = format!(
        "
        SELECT subscription, actor_profile AS sender
        FROM actor_profile
        JOIN subscription
        ON (actor_profile.id = subscription.sender_id)
        WHERE
            {filter}
            AND ($2::integer IS NULL OR subscription.id < $2)
        ORDER BY subscription.id DESC
        LIMIT $3
        ",
        filter=filter,
    );
    let rows = db_client.query(
        &statement,
        &[&recipient_id, &max_subscription_id, &i64::from(limit)],
    ).await?;
    let subscriptions = rows.iter()
        .map(Subscription::try_from)
        .collect::<Result<_, _>>()?;
    Ok(subscriptions)
}

pub async fn get_active_subscription_count(
    db_client: &impl DatabaseClient,
) -> Result<i64, DatabaseError> {
    // Only local (managed) recipients
    let row = db_client.query_one(
        "
        SELECT count(subscription)
        FROM subscription
        JOIN actor_profile ON (subscription.recipient_id = actor_profile.id)
        WHERE
            actor_profile.user_id IS NOT NULL
            AND expires_at > CURRENT_TIMESTAMP
        ",
        &[],
    ).await?;
    let count = row.try_get("count")?;
    Ok(count)
}

pub async fn get_expired_subscription_count(
    db_client: &impl DatabaseClient,
) -> Result<i64, DatabaseError> {
    // Only local (managed) recipients
    let row = db_client.query_one(
        "
        SELECT count(subscription)
        FROM subscription
        JOIN actor_profile ON (subscription.recipient_id = actor_profile.id)
        WHERE
            actor_profile.user_id IS NOT NULL
            AND expires_at <= CURRENT_TIMESTAMP
        ",
        &[],
    ).await?;
    let count = row.try_get("count")?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::profiles::{
        queries::create_profile,
        types::ProfileCreateData,
    };
    use crate::relationships::{
        queries::has_relationship,
        types::RelationshipType,
    };
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_subscription() {
        let db_client = &mut create_test_database().await;
        let sender_data = ProfileCreateData {
            username: "sender".to_string(),
            ..Default::default()
        };
        let sender = create_profile(db_client, sender_data).await.unwrap();
        let recipient_data = UserCreateData {
            username: "recipient".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let recipient = create_user(db_client, recipient_data).await.unwrap();
        let expires_at = Utc::now();
        let updated_at = Utc::now();

        let subscription = create_subscription(
            db_client,
            sender.id,
            recipient.id,
            expires_at,
            updated_at,
        ).await.unwrap();
        assert_eq!(subscription.sender_id, sender.id);
        assert_eq!(subscription.recipient_id, recipient.id);
        assert_eq!(
            subscription.expires_at.timestamp_millis(),
            expires_at.timestamp_millis(),
        );

        let is_subscribed = has_relationship(
            db_client,
            sender.id,
            recipient.id,
            RelationshipType::Subscription,
        ).await.unwrap();
        assert_eq!(is_subscribed, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_incoming_subscriptions() {
        let db_client = &mut create_test_database().await;
        let recipient_data = ProfileCreateData {
            username: "recipient".to_string(),
            ..Default::default()
        };
        let recipient =
            create_profile(db_client, recipient_data).await.unwrap();
        let results = get_incoming_subscriptions(
            db_client,
            recipient.id,
            true,
            None,
            40,
        ).await.unwrap();
        assert_eq!(results.is_empty(), true);
    }
}
