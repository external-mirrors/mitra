use uuid::Uuid;

use crate::{
    accounts::{
        queries::create_automated_account,
        types::{
            AutomatedAccountData,
            AutomatedAccountDetailed,
            AutomatedAccountType,
        },
    },
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
            has_relationship,
        },
        types::{RelatedActorProfile, RelationshipType},
    },
};

use super::types::GroupCreateData;

pub async fn create_group(
    db_client: &mut impl DatabaseClient,
    owner_id: Uuid,
    group_data: GroupCreateData,
) -> Result<AutomatedAccountDetailed, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let account_data = AutomatedAccountData {
        username: group_data.username,
        bio: group_data.bio,
        bio_source: group_data.bio_source,
        emojis: group_data.emojis,
        account_type: AutomatedAccountType::Group,
        rsa_secret_key: group_data.rsa_secret_key,
        ed25519_secret_key: group_data.ed25519_secret_key,
    };
    let account =
        create_automated_account(&mut transaction, account_data).await?;
    add_group_admin(&transaction, owner_id, account.id).await?;
    transaction.commit().await?;
    Ok(account)
}

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

pub async fn can_add_remove_admins(
    db_client: &impl DatabaseClient,
    member_id: Uuid,
    group_id: Uuid,
) -> Result<bool, DatabaseError> {
    // Power levels are not checked
    has_relationship(
        db_client,
        member_id,
        group_id,
        RelationshipType::GroupAdmin,
    ).await
}

pub async fn add_group_admin(
    db_client: &impl DatabaseClient,
    member_id: Uuid,
    group_id: Uuid,
) -> Result<(), DatabaseError> {
    create_relationship(
        db_client,
        member_id,
        group_id,
        RelationshipType::GroupAdmin,
    ).await
}

pub async fn remove_group_admin(
    db_client: &impl DatabaseClient,
    member_id: Uuid,
    group_id: Uuid,
) -> Result<(), DatabaseError> {
    delete_relationship(
        db_client,
        member_id,
        group_id,
        RelationshipType::GroupAdmin,
    ).await
}

const AFFILIATION_TYPES: [RelationshipType; 2] = [
    RelationshipType::GroupAdmin,
    RelationshipType::GroupMember,
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

pub async fn join_private_group(
    db_client: &impl DatabaseClient,
    member_id: Uuid,
    group_id: Uuid,
) -> Result<(), DatabaseError> {
    match create_relationship(
        db_client,
        member_id,
        group_id,
        RelationshipType::GroupMember,
    ).await {
        Ok(_) | Err(DatabaseError::AlreadyExists(_)) => (),
        Err(other_error) => return Err(other_error),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use apx_core::crypto::{
        eddsa::generate_weak_ed25519_key,
        rsa::generate_weak_rsa_key,
    };
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
    async fn test_create_group() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "user").await;
        let group_name = "tesgroup";
        let group_description = "my group";
        let group_data = GroupCreateData {
            username: group_name.to_owned(),
            bio: Some(group_description.to_owned()),
            bio_source: Some(group_description.to_owned()),
            emojis: vec![],
            rsa_secret_key: generate_weak_rsa_key().unwrap(),
            ed25519_secret_key: generate_weak_ed25519_key(),
        };
        let group = create_group(
            db_client,
            user.id,
            group_data,
        ).await.unwrap();
        assert_eq!(group.account_type, AutomatedAccountType::Group);
        let profile = group.profile;
        assert_eq!(profile.automated_account_id.is_some(), true);
        assert_eq!(profile.is_group(), true);
        assert_eq!(profile.username, group_name);
        assert_eq!(profile.bio.unwrap(), group_description);
        assert_eq!(profile.bio_source.unwrap(), group_description);
        let is_admin = has_relationship(
            db_client,
            user.id,
            group.id,
            RelationshipType::GroupAdmin,
        ).await.unwrap();
        assert_eq!(is_admin, true);
    }

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
