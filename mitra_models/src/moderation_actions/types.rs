use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModerationActionType {
    PostDeleted,
}

impl From<ModerationActionType> for i16 {
    fn from(value: ModerationActionType) -> i16 {
        match value {
            ModerationActionType::PostDeleted => 1,
        }
    }
}

impl TryFrom<i16> for ModerationActionType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let action_type = match value {
            1 => Self::PostDeleted,
            _ => return Err(DatabaseTypeError),
        };
        Ok(action_type)
    }
}

int_enum_from_sql!(ModerationActionType);
int_enum_to_sql!(ModerationActionType);

#[derive(FromSql)]
#[postgres(name = "moderation_action")]
pub struct ModerationAction {
    pub id: Uuid,
    pub moderator_id: Uuid,
    pub target_id: Uuid,
    pub action_type: ModerationActionType,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}
