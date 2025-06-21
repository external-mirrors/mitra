use std::num::NonZeroU64;

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
use mitra_validators::errors::ValidationError;

// TODO: should be based on network fee
pub const MONERO_PAYMENT_AMOUNT_MIN: u64 = 1000000000; // 0.001

const SECONDS_IN_MONTH: u64 = 30 * 24 * 3600;

pub fn validate_subscription_price(
    value: NonZeroU64,
) -> Result<(), ValidationError> {
    let price_per_second = value.get();
    let price_per_month = price_per_second.checked_mul(SECONDS_IN_MONTH)
        .ok_or(ValidationError("price is too high"))?;
    if price_per_month < MONERO_PAYMENT_AMOUNT_MIN {
        return Err(ValidationError("price is too low"));
    };
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_subscription_price() {
        let price = NonZeroU64::new(500).unwrap();
        let result = validate_subscription_price(price);
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_validate_subscription_price_too_small() {
        let price = NonZeroU64::new(350).unwrap();
        let result = validate_subscription_price(price);
        assert_eq!(result.is_ok(), false);
    }
}
