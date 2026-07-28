use uuid::Uuid;

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
    profiles::{
        queries::get_profile_by_id,
        types::DbActorProfile,
    },
    relationships::{
        queries::{
            get_related_combined,
        },
        types::{RelatedActorProfile, RelationshipType},
    },
};

pub async fn get_group_by_id(
    db_client: &impl DatabaseClient,
    group_id: Uuid,
) -> Result<DbActorProfile, DatabaseError> {
    let profile = get_profile_by_id(db_client, group_id).await?;
    if !profile.is_group() {
        return Err(DatabaseError::NotFound("group"));
    };
    Ok(profile)
}

const AFFILIATION_TYPES: [RelationshipType; 1] = [
    RelationshipType::GroupAdmin,
];

pub async fn get_affiliated_profiles(
    db_client: &impl DatabaseClient,
    group_id: Uuid,
) -> Result<Vec<RelatedActorProfile<i32>>, DatabaseError> {
    get_related_combined(
        db_client,
        group_id,
        &AFFILIATION_TYPES,
        false, // reverse relationships
    ).await
}
