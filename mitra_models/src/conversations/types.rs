use postgres_types::FromSql;
use uuid::Uuid;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};

pub(crate) const AP_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";

#[derive(Clone, FromSql)]
#[postgres(name = "conversation")]
pub struct Conversation {
    pub id: Uuid,
    pub root_id: Uuid,
    // "audience" is None when the conversation is direct,
    // or when it is limited and created by the database migration
    pub audience: Option<String>,
}

impl Conversation {
    pub fn is_public(&self) -> bool {
        self.audience.as_ref().is_some_and(|audience| audience == AP_PUBLIC)
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
