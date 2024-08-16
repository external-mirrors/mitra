use uuid::Uuid;

use crate::database::{
    DatabaseClient,
    DatabaseError,
};

use super::queries::{
    create_follow_request_unchecked,
    follow_request_rejected,
    get_follow_request_by_participants,
    has_relationship,
    unfollow,
};
use super::types::{DbFollowRequest, RelationshipType};

pub async fn create_follow_request(
    db_client: &mut impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Prevent changes to relationship table
    transaction.execute(
        "LOCK TABLE relationship IN EXCLUSIVE MODE",
        &[],
    ).await?;
    let is_following = has_relationship(
        &transaction,
        source_id,
        target_id,
        RelationshipType::Follow,
    ).await?;
    if is_following {
        // Follow request should not be created if
        // follow relationship exists.
        return Err(DatabaseError::AlreadyExists("relationship"));
    };
    let request = create_follow_request_unchecked(
        &transaction,
        source_id,
        target_id,
    ).await?;
    transaction.commit().await?;
    Ok(request)
}

pub async fn remove_follower(
    db_client: &mut impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<Option<String>, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let mut maybe_activity_id = None;
    match get_follow_request_by_participants(
        &transaction,
        source_id,
        target_id,
    ).await {
        Ok(follow_request) => {
            follow_request_rejected(
                &mut transaction,
                follow_request.id,
            ).await?;
            // NOTE: Old follow requests may not have activity ID
            maybe_activity_id = follow_request.activity_id;
        },
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error),
    };
    unfollow(&mut transaction, source_id, target_id).await?;
    transaction.commit().await?;
    Ok(maybe_activity_id)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        profiles::test_utils::create_test_remote_profile,
        relationships::queries::{
            create_remote_follow_request_opt,
            follow_request_accepted,
        },
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_remove_follower() {
        let db_client = &mut create_test_database().await;
        let target = create_test_user(db_client, "test").await;
        let source = create_test_remote_profile(
            db_client,
            "follower",
            "social.example",
            "https://social.example/1",
        ).await;
        let follow_activity_id = "https://social.example/activities/1";
        let follow_request = create_remote_follow_request_opt(
            db_client,
            source.id,
            target.id,
            &follow_activity_id,
        ).await.unwrap();
        follow_request_accepted(db_client, follow_request.id).await.unwrap();
        let maybe_activity_id =
            remove_follower(db_client, source.id, target.id).await.unwrap();

        assert_eq!(maybe_activity_id.unwrap(), follow_activity_id);
        let is_following = has_relationship(
            db_client,
            source.id,
            target.id,
            RelationshipType::Follow,
        ).await.unwrap();
        assert_eq!(is_following, false);
        let is_rejected = has_relationship(
            db_client,
            target.id,
            source.id,
            RelationshipType::Reject,
        ).await.unwrap();
        assert_eq!(is_rejected, true);
    }
}
