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
            create_relationship,
            delete_relationship,
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

pub async fn update_affiliations(
    db_client: &mut impl DatabaseClient,
    group_id: Uuid,
    mut affiliations: Vec<(Uuid, RelationshipType)>,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    transaction.execute(
        "LOCK TABLE relationship IN EXCLUSIVE MODE",
        &[],
    ).await?;
    let existing_affiliations =
        get_affiliated_profiles(&transaction, group_id).await?;
    let mut deleted_affiliations = vec![];
    for related_profile in existing_affiliations {
        let affiliation =
            (related_profile.profile.id, related_profile.relationship_type);
        if affiliations.contains(&affiliation) {
            affiliations.retain(|item| item != &affiliation);
        } else {
            deleted_affiliations.push(affiliation);
        };
    };
    for (source_id, relationship_type) in deleted_affiliations {
        delete_relationship(
            &transaction,
            source_id,
            group_id,
            relationship_type,
        ).await?;
    };
    for (source_id, relationship_type) in affiliations {
        if !AFFILIATION_TYPES.contains(&relationship_type) {
            return Err(DatabaseError::type_error());
        };
        create_relationship(
            &transaction,
            source_id,
            group_id,
            relationship_type,
        ).await?;
    };
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        accounts::test_utils::create_test_user,
        database::{
            test_utils::create_test_database,
        },
        relationships::queries::has_relationship,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_update_affiliations() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "user").await;
        let group = create_test_user(db_client, "group").await;
        // Add affiliations
        let affiliations = vec![
            (user.id, RelationshipType::GroupAdmin),
        ];
        update_affiliations(
            db_client,
            group.id,
            affiliations,
        ).await.unwrap();
        let is_admin = has_relationship(
            db_client,
            user.id,
            group.id,
            RelationshipType::GroupAdmin,
        ).await.unwrap();
        assert_eq!(is_admin, true);
        // Remove affiliations
        update_affiliations(
            db_client,
            group.id,
            vec![],
        ).await.unwrap();
        let is_admin = has_relationship(
            db_client,
            user.id,
            group.id,
            RelationshipType::GroupAdmin,
        ).await.unwrap();
        assert_eq!(is_admin, false);
    }
}
