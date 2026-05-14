use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseError,
    DatabaseTypeError,
};
use crate::emojis::types::CustomEmoji;
use crate::posts::types::{Post, PostDetailed};
use crate::profiles::types::DbActorProfile;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventType {
    Follow,
    FollowRequest,
    Reply,
    Reaction,
    Mention,
    Repost,
    SubscriberPayment,
    SubscriptionStart,
    SubscriptionExpiration,
    Move,
    SignUp,
    SubscriberLeaving,
}

impl From<EventType> for i16 {
    fn from(value: EventType) -> i16 {
        match value {
            EventType::Follow => 1,
            EventType::FollowRequest => 2,
            EventType::Reply => 3,
            EventType::Reaction => 4,
            EventType::Mention => 5,
            EventType::Repost => 6,
            EventType::SubscriberPayment => 7,
            EventType::SubscriptionStart => unimplemented!("not supported"),
            EventType::SubscriptionExpiration => 9,
            EventType::Move => 10,
            EventType::SignUp => 11,
            EventType::SubscriberLeaving => 12,
        }
    }
}

impl TryFrom<i16> for EventType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let event_type = match value {
            1 => Self::Follow,
            2 => Self::FollowRequest,
            3 => Self::Reply,
            4 => Self::Reaction,
            5 => Self::Mention,
            6 => Self::Repost,
            7 => Self::SubscriberPayment,
            8 => Self::SubscriptionStart,
            9 => Self::SubscriptionExpiration,
            10 => Self::Move,
            11 => Self::SignUp,
            12 => Self::SubscriberLeaving,
            _ => return Err(DatabaseTypeError),
        };
        Ok(event_type)
    }
}

int_enum_from_sql!(EventType);
int_enum_to_sql!(EventType);

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "notification")]
struct Notification {
    id: i32,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Option<Uuid>,
    reaction_id: Option<Uuid>,
    invoice_id: Option<Uuid>,
    event_type: EventType,
    created_at: DateTime<Utc>,
}

pub struct NotificationDetailed {
    pub id: i32,
    pub sender: DbActorProfile,
    pub post: Option<PostDetailed>,
    pub reaction_content: Option<String>,
    pub reaction_emoji: Option<CustomEmoji>,
    pub payment_amount: Option<i64>,
    pub event_type: EventType,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<&Row> for NotificationDetailed {

    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_notification: Notification = row.try_get("notification")?;
        let db_sender: DbActorProfile = row.try_get("sender")?;
        let maybe_db_post: Option<Post> = row.try_get("post")?;
        let maybe_post = match maybe_db_post {
            Some(_) => {
                let post = PostDetailed::try_from(row)?;
                Some(post)
            },
            None => None,
        };
        let maybe_reaction_content = row.try_get("reaction_content")?;
        let maybe_reaction_emoji = row.try_get("reaction_emoji")?;
        let maybe_payment_amount = row.try_get("payment_amount")?;
        let notification = Self {
            id: db_notification.id,
            sender: db_sender,
            post: maybe_post,
            reaction_content: maybe_reaction_content,
            reaction_emoji: maybe_reaction_emoji,
            payment_amount: maybe_payment_amount,
            event_type: db_notification.event_type,
            created_at: db_notification.created_at,
        };
        notification.sender.check_consistency()?;
        Ok(notification)
    }
}
