use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::database::DatabaseError;
use crate::profiles::types::DbActorProfile;

#[derive(FromSql)]
#[postgres(name = "subscription")]
pub struct Subscription {
    pub id: i32,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct SubscriptionDetailed {
    pub id: i32,
    pub sender: DbActorProfile,
    pub expires_at: DateTime<Utc>,
}

impl TryFrom<&Row> for SubscriptionDetailed {

    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_subscription: Subscription = row.try_get("subscription")?;
        let db_sender: DbActorProfile = row.try_get("sender")?;
        let subscription = Self {
            id: db_subscription.id,
            sender: db_sender,
            expires_at: db_subscription.expires_at,
        };
        subscription.sender.check_consistency()?;
        Ok(subscription)
    }
}
