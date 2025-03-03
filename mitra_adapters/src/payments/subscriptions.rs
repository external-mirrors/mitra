use chrono::{DateTime, Duration, Utc};

use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
    subscriptions::queries::{
        create_subscription,
        get_subscription_by_participants,
        update_subscription,
    },
    subscriptions::types::DbSubscription,
    users::types::User,
};

pub async fn create_or_update_subscription(
    db_client: &mut impl DatabaseClient,
    sender: &DbActorProfile,
    recipient: &DbActorProfile,
    next_expires_at: impl Fn(Option<DateTime<Utc>>) -> DateTime<Utc>,
) -> Result<DbSubscription, DatabaseError> {
    let subscription = match get_subscription_by_participants(
        db_client,
        sender.id,
        recipient.id,
    ).await {
        Ok(subscription) => {
            // Update subscription expiration date
            let expires_at = next_expires_at(Some(subscription.expires_at));
            let subscription = update_subscription(
                db_client,
                subscription.id,
                expires_at,
                Utc::now(),
            ).await?;
            log::info!(
                "subscription updated: {0} to {1}",
                sender,
                recipient,
            );
            subscription
        },
        Err(DatabaseError::NotFound(_)) => {
            // New subscription
            let expires_at = next_expires_at(None);
            let subscription = create_subscription(
                db_client,
                sender.id,
                recipient.id,
                expires_at,
                Utc::now(),
            ).await?;
            log::info!(
                "subscription created: {0} to {1}",
                sender,
                recipient,
            );
            subscription
        },
        Err(other_error) => return Err(other_error),
    };
    Ok(subscription)
}

pub async fn create_or_update_local_subscription(
    db_client: &mut impl DatabaseClient,
    sender: &DbActorProfile,
    recipient: &User,
    duration_secs: i64,
) -> Result<DbSubscription, DatabaseError> {
    create_or_update_subscription(
        db_client,
        sender,
        &recipient.profile,
        |maybe_expires_at| {
            if let Some(expires_at) = maybe_expires_at {
                std::cmp::max(expires_at, Utc::now()) +
                    Duration::seconds(duration_secs)
            } else {
                Utc::now() + Duration::seconds(duration_secs)
            }
        },
    ).await
}
