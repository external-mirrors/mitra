use postgres_types::FromSql;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterAction {
    Reject,
    RejectMediaAttachments,
    RejectProfileImages,
}

impl From<FilterAction> for i16 {
    fn from(value: FilterAction) -> i16 {
        match value {
            FilterAction::Reject => 1,
            FilterAction::RejectMediaAttachments => 2,
            FilterAction::RejectProfileImages => 3,
        }
    }
}

impl TryFrom<i16> for FilterAction {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let action = match value {
            1 => Self::Reject,
            2 => Self::RejectMediaAttachments,
            3 => Self::RejectProfileImages,
            _ => return Err(DatabaseTypeError),
        };
        Ok(action)
    }
}

int_enum_from_sql!(FilterAction);
int_enum_to_sql!(FilterAction);

#[derive(FromSql)]
#[postgres(name = "filter_rule")]
pub struct FilterRule {
    #[allow(dead_code)]
    id: i32,
    pub target: String,
    pub filter_action: FilterAction,
    pub is_reversed: bool,
}
