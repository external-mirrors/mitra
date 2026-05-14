use tokio_postgres::Row;
use postgres_types::FromSql;
use uuid::Uuid;

use crate::{
    activitypub::constants::AP_PUBLIC,
    database::{
        int_enum::{int_enum_from_sql, int_enum_to_sql},
        DatabaseError,
        DatabaseTypeError,
    },
    posts::types::PostDetailed,
    profiles::types::DbActorProfile,
};

#[derive(Clone, FromSql)]
#[postgres(name = "conversation")]
pub struct Conversation {
    pub id: Uuid,
    pub root_id: Uuid,
    // Conversation is managed when the root is managed
    pub is_managed: bool,
    // "object_id" is None when the conversation is managed (local),
    // or when the ID is not known.
    pub object_id: Option<String>,
    // "audience" is None when the conversation is direct,
    // or when it is limited and created by the database migration
    pub audience: Option<String>,
}

impl Conversation {
    pub fn is_public(&self) -> bool {
        self.audience.as_ref().is_some_and(|audience| audience == AP_PUBLIC)
    }
}

pub struct ConversationPreview {
    pub conversation: Conversation,
    pub participants: Vec<DbActorProfile>,
    pub last_post: PostDetailed,
}

impl TryFrom<&Row> for ConversationPreview {
    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let conversation = row.try_get("conversation")?;
        let participants = row.try_get("participants")?;
        let last_post = PostDetailed::try_from(row)?;
        let conversation_preview = Self {
            conversation,
            participants,
            last_post,
        };
        Ok(conversation_preview)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackingStatus {
    Follow,
}

impl From<TrackingStatus> for i16 {
    fn from(value: TrackingStatus) -> i16 {
        match value {
            TrackingStatus::Follow => 1,
        }
    }
}

impl TryFrom<i16> for TrackingStatus {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let tracking_status = match value {
            1 => Self::Follow,
            _ => return Err(DatabaseTypeError),
        };
        Ok(tracking_status)
    }
}

int_enum_from_sql!(TrackingStatus);
int_enum_to_sql!(TrackingStatus);
