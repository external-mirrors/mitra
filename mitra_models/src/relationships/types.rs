use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::{
    database::{
        int_enum::{int_enum_from_sql, int_enum_to_sql},
        DatabaseError,
        DatabaseTypeError,
    },
    profiles::types::DbActorProfile,
};

#[derive(Debug)]
pub enum RelationshipType {
    Follow,
    FollowRequest, // follow_request table
    Subscription,
    HideReposts,
    HideReplies,
    Mute,
    Reject, // follow request rejected
}

impl From<&RelationshipType> for i16 {
    fn from(value: &RelationshipType) -> i16 {
        match value {
            RelationshipType::Follow => 1,
            RelationshipType::FollowRequest => 2,
            RelationshipType::Subscription => 3,
            RelationshipType::HideReposts => 4,
            RelationshipType::HideReplies => 5,
            RelationshipType::Mute => 6,
            RelationshipType::Reject => 7,
        }
    }
}

impl TryFrom<i16> for RelationshipType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let relationship_type = match value {
            1 => Self::Follow,
            2 => Self::FollowRequest,
            3 => Self::Subscription,
            4 => Self::HideReposts,
            5 => Self::HideReplies,
            6 => Self::Mute,
            7 => Self::Reject,
            _ => return Err(DatabaseTypeError),
        };
        Ok(relationship_type)
    }
}

int_enum_from_sql!(RelationshipType);
int_enum_to_sql!(RelationshipType);

pub struct DbRelationship {
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub relationship_type: RelationshipType,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
}

impl DbRelationship {
    pub fn is_direct(
        &self,
        source_id: Uuid,
        target_id: Uuid,
    ) -> Result<bool, DatabaseTypeError> {
        if self.source_id == source_id && self.target_id == target_id {
            Ok(true)
        } else if self.source_id == target_id && self.target_id == source_id {
            Ok(false)
        } else {
            Err(DatabaseTypeError)
        }
    }

    pub(super) fn with(
        &self,
        profile_id: Uuid,
    ) -> Result<Uuid, DatabaseTypeError> {
        if self.source_id == profile_id {
            // Direct relationship
            Ok(self.target_id)
        } else if self.target_id == profile_id {
            // Inverse relationship
            Ok(self.source_id)
        } else {
            Err(DatabaseTypeError)
        }
    }
}

impl TryFrom<&Row> for DbRelationship {

    type Error = tokio_postgres::Error;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let relationship = Self {
            source_id: row.try_get("source_id")?,
            target_id: row.try_get("target_id")?,
            relationship_type: row.try_get("relationship_type")?,
            created_at: row.try_get("created_at")?,
        };
        Ok(relationship)
    }
}

pub struct RelatedActorProfile<T> {
    pub related_id: T,
    pub profile: DbActorProfile,
}

impl<T> TryFrom<&Row> for RelatedActorProfile<T>
    where for<'sql> T: FromSql<'sql>
{
    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let related_id: T = row.try_get("id")?;
        let profile = row.try_get("actor_profile")?;
        Ok(Self { related_id, profile })
    }
}

#[derive(Debug, PartialEq)]
pub enum FollowRequestStatus {
    Pending,
    Accepted,
    #[deprecated]
    Rejected,
}

impl From<&FollowRequestStatus> for i16 {
    fn from(value: &FollowRequestStatus) -> i16 {
        match value {
            FollowRequestStatus::Pending  => 1,
            FollowRequestStatus::Accepted => 2,
            #[allow(deprecated)]
            FollowRequestStatus::Rejected => 3,
        }
    }
}

impl TryFrom<i16> for FollowRequestStatus {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let status = match value {
            1 => Self::Pending,
            2 => Self::Accepted,
            #[allow(deprecated)]
            3 => Self::Rejected,
            _ => return Err(DatabaseTypeError),
        };
        Ok(status)
    }
}

int_enum_from_sql!(FollowRequestStatus);
int_enum_to_sql!(FollowRequestStatus);

#[derive(FromSql)]
#[postgres(name = "follow_request")]
pub struct DbFollowRequest {
    pub id: Uuid,
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub activity_id: Option<String>,
    pub request_status: FollowRequestStatus,
    #[allow(dead_code)]
    has_deprecated_ap_id: bool,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
}
