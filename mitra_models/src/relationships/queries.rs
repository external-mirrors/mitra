use std::collections::HashMap;

use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
    DatabaseTypeError,
};
use crate::notifications::helpers::create_follow_notification;
use crate::profiles::{
    queries::{
        update_follower_count,
        update_following_count,
        update_subscriber_count,
    },
    types::DbActorProfile,
};

use super::types::{
    DbFollowRequest,
    DbRelationship,
    FollowRequestStatus,
    RelatedActorProfile,
    RelationshipType,
};

pub async fn get_relationships(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<Vec<DbRelationship>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT source_id, target_id, relationship_type, created_at
        FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            OR
            source_id = $2 AND target_id = $1
        UNION ALL
        SELECT source_id, target_id, $4, created_at
        FROM follow_request
        WHERE
            (
                source_id = $1 AND target_id = $2
                OR
                source_id = $2 AND target_id = $1
            )
            AND request_status = $3
        ",
        &[
            &source_id,
            &target_id,
            &FollowRequestStatus::Pending,
            &RelationshipType::FollowRequest,
        ],
    ).await?;
    let relationships = rows.iter()
        .map(DbRelationship::try_from)
        .collect::<Result<_, _>>()?;
    Ok(relationships)
}

pub async fn get_relationships_many(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_ids: &[Uuid],
) -> Result<Vec<(Uuid, Vec<DbRelationship>)>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT source_id, target_id, relationship_type, created_at
        FROM relationship
        WHERE
            source_id = $1 AND target_id = ANY($2)
            OR
            source_id = ANY($2) AND target_id = $1
        UNION ALL
        SELECT source_id, target_id, $4, created_at
        FROM follow_request
        WHERE
            (
                source_id = $1 AND target_id = ANY($2)
                OR
                source_id = ANY($2) AND target_id = $1
            )
            AND request_status = $3
        ",
        &[
            &source_id,
            &target_ids,
            &FollowRequestStatus::Pending,
            &RelationshipType::FollowRequest,
        ],
    ).await?;
    // No duplicate keys in buckets hashmap
    let mut buckets: HashMap<Uuid, Vec<DbRelationship>> =
        HashMap::from_iter(target_ids.iter().map(|id| (*id, vec![])));
    for row in rows {
        let relationship = DbRelationship::try_from(&row)?;
        let target_id = relationship.with(source_id)?;
        let target_relationships = buckets.get_mut(&target_id)
            .ok_or(DatabaseTypeError)?;
        target_relationships.push(relationship);
    };
    Ok(buckets.into_iter().collect())
}

pub async fn has_relationship(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
    relationship_type: RelationshipType,
) -> Result<bool, DatabaseError> {
    if matches!(relationship_type, RelationshipType::FollowRequest) {
        return Err(DatabaseTypeError.into());
    };
    let maybe_row = db_client.query_opt(
        "
        SELECT 1
        FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[
            &source_id,
            &target_id,
            &relationship_type,
        ],
    ).await?;
    Ok(maybe_row.is_some())
}

async fn get_related_paginated(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    relationship_type: RelationshipType,
    is_direct: bool,
    max_relationship_id: Option<i32>,
    limit: u16,
) -> Result<Vec<RelatedActorProfile<i32>>, DatabaseError> {
    let statement = format!(
        "
        SELECT relationship.id, actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.{target_id})
        WHERE
            relationship.{source_id} = $1
            AND relationship.relationship_type = $2
            AND ($3::integer IS NULL OR relationship.id < $3)
        ORDER BY relationship.id DESC
        LIMIT $4
        ",
        source_id=if is_direct { "source_id" } else { "target_id" },
        target_id=if is_direct { "target_id" } else { "source_id" },
    );
    let rows = db_client.query(
        &statement,
        &[
            &source_id,
            &relationship_type,
            &max_relationship_id,
            &i64::from(limit),
        ],
    ).await?;
    let related_profiles = rows.iter()
        .map(RelatedActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(related_profiles)
}

pub async fn follow(
    db_client: &mut impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    transaction.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ",
        &[&source_id, &target_id, &RelationshipType::Follow],
    ).await.map_err(catch_unique_violation("relationship"))?;
    transaction.execute(
        "
        DELETE FROM relationship
        WHERE source_id = $1 AND target_id = $2 AND relationship_type = $3
        ",
        &[&target_id, &source_id, &RelationshipType::Reject],
    ).await?;
    let target_profile = update_follower_count(&transaction, target_id, 1).await?;
    update_following_count(&transaction, source_id, 1).await?;
    if target_profile.is_local() && !target_profile.manually_approves_followers {
        create_follow_notification(&transaction, source_id, target_id).await?;
    };
    transaction.commit().await?;
    Ok(())
}

/// Deletes both a relationship and a corresponding follow request
pub async fn unfollow(
    db_client: &mut impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<Option<(Uuid, bool)>, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Delete relationship
    let deleted_count = transaction.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::Follow],
    ).await?;
    let relationship_deleted = deleted_count > 0;
    // Delete follow request
    let follow_request_deleted = delete_follow_request_opt(
        &transaction,
        source_id,
        target_id,
    ).await?;
    if !relationship_deleted && follow_request_deleted.is_none() {
        return Err(DatabaseError::NotFound("relationship"));
    };
    if relationship_deleted {
        // Also reset repost and reply visibility settings
        show_reposts(&transaction, source_id, target_id).await?;
        show_replies(&transaction, source_id, target_id).await?;
        // Update counters only if relationship existed
        update_follower_count(&transaction, target_id, -1).await?;
        update_following_count(&transaction, source_id, -1).await?;
    };
    transaction.commit().await?;
    Ok(follow_request_deleted)
}

pub(super) async fn create_follow_request_unchecked(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let request_id = generate_ulid();
    let row = db_client.query_one(
        "
        INSERT INTO follow_request (
            id, source_id, target_id, request_status
        )
        VALUES ($1, $2, $3, $4)
        RETURNING follow_request
        ",
        &[
            &request_id,
            &source_id,
            &target_id,
            &FollowRequestStatus::Pending,
        ],
    ).await.map_err(catch_unique_violation("follow request"))?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

/// Save follow request from remote actor
pub async fn create_remote_follow_request_opt(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
    activity_id: &str,
) -> Result<DbFollowRequest, DatabaseError> {
    let request_id = generate_ulid();
    // Update activity ID if follow request already exists
    let row = db_client.query_one(
        "
        INSERT INTO follow_request (
            id,
            source_id,
            target_id,
            activity_id,
            request_status
        )
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (source_id, target_id)
        DO UPDATE SET activity_id = $4
        RETURNING follow_request
        ",
        &[
            &request_id,
            &source_id,
            &target_id,
            &activity_id,
            &FollowRequestStatus::Pending,
        ],
    ).await?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn follow_request_accepted(
    db_client: &mut impl DatabaseClient,
    request_id: Uuid,
) -> Result<(), DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        UPDATE follow_request
        SET request_status = $1
        WHERE id = $2
        RETURNING source_id, target_id
        ",
        &[&FollowRequestStatus::Accepted, &request_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let source_id: Uuid = row.try_get("source_id")?;
    let target_id: Uuid = row.try_get("target_id")?;
    follow(&mut transaction, source_id, target_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn follow_request_rejected(
    db_client: &mut impl DatabaseClient,
    request_id: Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        DELETE FROM follow_request
        WHERE id = $1
        RETURNING source_id, target_id
        ",
        &[&request_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let source_id: Uuid = row.try_get("source_id")?;
    let target_id: Uuid = row.try_get("target_id")?;
    // Make rejection record
    transaction.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&target_id, &source_id, &RelationshipType::Reject],
    ).await?;
    transaction.commit().await?;
    Ok(())
}

async fn delete_follow_request_opt(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<Option<(Uuid, bool)>, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        DELETE FROM follow_request
        WHERE source_id = $1 AND target_id = $2
        RETURNING
            follow_request.id,
            follow_request.has_deprecated_ap_id
        ",
        &[&source_id, &target_id],
    ).await?;
    if let Some(row) = maybe_row {
        let request_id = row.try_get("id")?;
        let request_has_deprecated_ap_id = row.try_get("has_deprecated_ap_id")?;
        Ok(Some((request_id, request_has_deprecated_ap_id)))
    } else {
        Ok(None)
    }
}

pub async fn get_follow_request_by_id(
    db_client:  &impl DatabaseClient,
    request_id: Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT follow_request
        FROM follow_request
        WHERE id = $1
        ",
        &[&request_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn get_follow_request_by_remote_activity_id(
    db_client: &impl DatabaseClient,
    activity_id: &str,
) -> Result<DbFollowRequest, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT follow_request
        FROM follow_request
        WHERE activity_id = $1
        ",
        &[&activity_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn get_follow_request_by_participants(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT follow_request
        FROM follow_request
        WHERE source_id = $1 AND target_id = $2
        ",
        &[&source_id, &target_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn get_followers(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.source_id)
        WHERE
            relationship.target_id = $1
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_followers_paginated(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    max_relationship_id: Option<i32>,
    limit: u16,
) -> Result<Vec<RelatedActorProfile<i32>>, DatabaseError> {
    get_related_paginated(
        db_client,
        profile_id,
        RelationshipType::Follow,
        false, // reverse
        max_relationship_id,
        limit,
    ).await
}

/// Returns true if actor has an account,
/// or has local followers or follow requests
pub async fn is_local_or_followed(
    db_client: &impl DatabaseClient,
    actor_id: &str,
) -> Result<bool, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1
        FROM actor_profile
        WHERE
            actor_profile.actor_id = $1
            AND (
                EXISTS (
                    SELECT 1 FROM relationship
                    WHERE target_id = actor_profile.id AND relationship_type = $2
                )
                OR EXISTS (
                    SELECT 1 FROM follow_request
                    WHERE target_id = actor_profile.id
                )
                OR actor_profile.user_id IS NOT NULL
                OR actor_profile.portable_user_id IS NOT NULL
            )
        ",
        &[&actor_id, &RelationshipType::Follow]
    ).await?;
    Ok(maybe_row.is_some())
}

pub async fn get_following(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.target_id)
        WHERE
            relationship.source_id = $1
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_following_paginated(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    max_relationship_id: Option<i32>,
    limit: u16,
) -> Result<Vec<RelatedActorProfile<i32>>, DatabaseError> {
    get_related_paginated(
        db_client,
        profile_id,
        RelationshipType::Follow,
        true, // direct
        max_relationship_id,
        limit,
    ).await
}

/// Returns incoming follow requests
pub async fn get_follow_requests_paginated(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    max_request_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<RelatedActorProfile<Uuid>>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT follow_request.id, actor_profile
        FROM actor_profile
        JOIN follow_request
        ON (actor_profile.id = follow_request.source_id)
        WHERE
            follow_request.target_id = $1
            AND follow_request.request_status = $2
            AND ($3::uuid IS NULL OR follow_request.id < $3)
        ORDER BY follow_request.id DESC
        LIMIT $4
        ",
        &[
            &profile_id,
            &FollowRequestStatus::Pending,
            &max_request_id,
            &i64::from(limit),
        ],
    ).await?;
    let related_profiles = rows.iter()
        .map(RelatedActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(related_profiles)
}

pub async fn subscribe(
    db_client: &mut impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    transaction.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await.map_err(catch_unique_violation("relationship"))?;
    update_subscriber_count(&transaction, target_id, 1).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn subscribe_opt(
    db_client: &mut impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let inserted_count = transaction.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await?;
    if inserted_count > 0 {
        update_subscriber_count(&transaction, target_id, 1).await?;
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn unsubscribe(
    db_client: &mut impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let deleted_count = transaction.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("relationship"));
    };
    update_subscriber_count(&transaction, target_id, -1).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn get_subscribers(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.source_id)
        WHERE
            relationship.target_id = $1
            AND relationship.relationship_type = $2
        ORDER BY relationship.id DESC
        ",
        &[&profile_id, &RelationshipType::Subscription],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn hide_reposts(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&source_id, &target_id, &RelationshipType::HideReposts],
    ).await?;
    Ok(())
}

pub async fn show_reposts(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    // Does not return NotFound error
    db_client.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::HideReposts],
    ).await?;
    Ok(())
}

pub async fn hide_replies(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&source_id, &target_id, &RelationshipType::HideReplies],
    ).await?;
    Ok(())
}

pub async fn show_replies(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    // Does not return NotFound error
    db_client.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::HideReplies],
    ).await?;
    Ok(())
}

pub async fn mute(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client
        .execute(
            "
            INSERT INTO relationship (source_id, target_id, relationship_type)
            VALUES ($1, $2, $3)
            ",
            &[&source_id, &target_id, &RelationshipType::Mute],
        )
        .await
        .map_err(
            catch_unique_violation("mute"),
        )?;
    Ok(())
}

pub async fn unmute(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client
        .query_opt(
            "
            DELETE FROM relationship
            WHERE
                source_id = $1 AND target_id = $2
                AND relationship_type = $3
            RETURNING relationship.id
            ",
            &[&source_id, &target_id, &RelationshipType::Mute],
        )
        .await?
        .ok_or(
            DatabaseError::NotFound("mute"),
        )?;
    Ok(())
}

pub async fn get_mutes_paginated(
    db_client: &impl DatabaseClient,
    source_id: Uuid,
    max_relationship_id: Option<i32>,
    limit: u16,
) -> Result<Vec<RelatedActorProfile<i32>>, DatabaseError> {
    get_related_paginated(
        db_client,
        source_id,
        RelationshipType::Mute,
        true, // direct
        max_relationship_id,
        limit,
    ).await
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::{
        test_utils::create_test_database,
        DatabaseError,
    };
    use crate::profiles::test_utils::create_test_remote_profile;
    use crate::users::test_utils::create_test_user;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_get_relationships() {
        let db_client = &mut create_test_database().await;
        let source = create_test_user(db_client, "source").await;
        let target = create_test_user(db_client, "target").await;
        follow(db_client, source.id, target.id).await.unwrap();

        let relationships = get_relationships(
            db_client,
            source.id,
            target.id,
        ).await.unwrap();
        assert_eq!(relationships.len(), 1);
        let relationship = &relationships[0];
        assert_eq!(relationship.source_id, source.id);
        assert_eq!(relationship.target_id, target.id);

        let relationships = get_relationships_many(
            db_client,
            source.id,
            &[target.id],
        ).await.unwrap();
        assert_eq!(relationships.len(), 1);
        let (target_id, target_relationships) = &relationships[0];
        assert_eq!(*target_id, target.id);
        assert_eq!(target_relationships.len(), 1);
        let relationship = &target_relationships[0];
        assert_eq!(relationship.source_id, source.id);
        assert_eq!(relationship.target_id, target.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_follow_remote_actor() {
        let db_client = &mut create_test_database().await;
        let source = create_test_user(db_client, "test").await;
        let target_actor_id = "https://social.example/users/1";
        let target = create_test_remote_profile(
            db_client,
            "followed",
            "social.example",
            target_actor_id,
        ).await;
        // Create follow request
        let follow_request = create_follow_request_unchecked(
            db_client,
            source.id,
            target.id,
        ).await.unwrap();
        assert_eq!(follow_request.source_id, source.id);
        assert_eq!(follow_request.target_id, target.id);
        assert_eq!(follow_request.activity_id, None);
        assert_eq!(follow_request.request_status, FollowRequestStatus::Pending);
        let following = get_following(db_client, source.id).await.unwrap();
        assert!(following.is_empty());
        // Accept follow request
        follow_request_accepted(db_client, follow_request.id).await.unwrap();
        let follow_request = get_follow_request_by_id(db_client, follow_request.id)
            .await.unwrap();
        assert_eq!(follow_request.request_status, FollowRequestStatus::Accepted);
        let following = get_following(db_client, source.id).await.unwrap();
        assert_eq!(following[0].id, target.id);
        let target_has_followers =
            is_local_or_followed(db_client, target_actor_id).await.unwrap();
        assert_eq!(target_has_followers, true);

        // Unfollow
        let (follow_request_id, follow_request_has_deprecated_ap_id) =
            unfollow(db_client, source.id, target.id).await.unwrap().unwrap();
        assert_eq!(follow_request_id, follow_request.id);
        assert_eq!(follow_request_has_deprecated_ap_id, false);
        let follow_request_result =
            get_follow_request_by_id(db_client, follow_request_id).await;
        assert!(matches!(
            follow_request_result,
            Err(DatabaseError::NotFound("follow request")),
        ));
        let following = get_following(db_client, source.id).await.unwrap();
        assert!(following.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn test_follow_remote_actor_rejected() {
        let db_client = &mut create_test_database().await;
        let source = create_test_user(db_client, "test").await;
        let target = create_test_remote_profile(
            db_client,
            "followed",
            "social.example",
            "https://social.example/users/1",
        ).await;
        // Create follow request
        let follow_request = create_follow_request_unchecked(
            db_client,
            source.id,
            target.id,
        ).await.unwrap();
        // Reject follow request
        follow_request_rejected(db_client, follow_request.id).await.unwrap();

        let result = get_follow_request_by_id(
            db_client,
            follow_request.id,
        ).await;
        assert!(matches!(
            result,
            Err(DatabaseError::NotFound("follow request")),
        ));
        let is_rejected = has_relationship(
            db_client,
            target.id,
            source.id,
            RelationshipType::Reject,
        ).await.unwrap();
        assert_eq!(is_rejected, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_followed_by_remote_actor() {
        let db_client = &mut create_test_database().await;
        let source = create_test_remote_profile(
            db_client,
            "follower",
            "social.example",
            "https://social.example/1",
        ).await;
        let target = create_test_user(db_client, "test").await;

        // Create follow request
        let activity_id = "https://example.org/objects/123";
        let _follow_request = create_remote_follow_request_opt(
            db_client,
            source.id,
            target.id,
            activity_id,
        ).await.unwrap();
        // Repeat
        let follow_request = create_remote_follow_request_opt(
            db_client,
            source.id,
            target.id,
            activity_id,
        ).await.unwrap();
        assert_eq!(follow_request.source_id, source.id);
        assert_eq!(follow_request.target_id, target.id);
        assert_eq!(follow_request.activity_id, Some(activity_id.to_string()));
        assert_eq!(follow_request.request_status, FollowRequestStatus::Pending);
        // Accept follow request
        follow_request_accepted(db_client, follow_request.id).await.unwrap();
        let follow_request = get_follow_request_by_id(db_client, follow_request.id)
            .await.unwrap();
        assert_eq!(follow_request.request_status, FollowRequestStatus::Accepted);

        // Another request received
        let activity_id = "https://social.example/objects/125";
        let follow_request_updated = create_remote_follow_request_opt(
            db_client,
            source.id,
            target.id,
            activity_id,
        ).await.unwrap();
        assert_eq!(follow_request_updated.id, follow_request.id);
        assert_eq!(
            follow_request_updated.activity_id.unwrap(),
            activity_id,
        );
        assert_eq!(
            follow_request_updated.request_status,
            FollowRequestStatus::Accepted,
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_get_following_paginated() {
        let db_client = &mut create_test_database().await;
        let source = create_test_user(db_client, "source").await;
        let target = create_test_user(db_client, "target").await;
        follow(db_client, source.id, target.id).await.unwrap();
        let results = get_following_paginated(
            db_client,
            source.id,
            None,
            10,
        ).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].profile.id, target.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_follow_requests_paginated() {
        let db_client = &mut create_test_database().await;
        let source = create_test_user(db_client, "source").await;
        let target = create_test_user(db_client, "target").await;
        let follow_request = create_follow_request_unchecked(
            db_client,
            source.id,
            target.id,
        ).await.unwrap();
        let results = get_follow_requests_paginated(
            db_client,
            target.id,
            None,
            10,
        ).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].related_id, follow_request.id);
        assert_eq!(results[0].profile.id, source.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_mute_and_unmute_actor() {
        let db_client = &mut create_test_database().await;
        let source = create_test_user(db_client, "source").await;
        let target = create_test_user(db_client, "target").await;
        // Mute
        mute(db_client, source.id, target.id).await.unwrap();
        assert!(
            has_relationship(
                db_client,
                source.id,
                target.id,
                RelationshipType::Mute
            ).await.unwrap()
        );
        // Unmute
        unmute(db_client, source.id, target.id).await.unwrap();
        assert!(
            !has_relationship(
                db_client,
                source.id,
                target.id,
                RelationshipType::Mute
            ).await.unwrap()
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_is_local_or_followed() {
        let db_client = &mut create_test_database().await;
        let actor_id = "https://social.example/1";
        let target = create_test_remote_profile(
            db_client,
            "target",
            "social.example",
            actor_id,
        ).await;
        let result = is_local_or_followed(db_client, actor_id).await.unwrap();
        assert_eq!(result, false);

        let source = create_test_user(db_client, "source").await;
        create_follow_request_unchecked(
            db_client,
            source.id,
            target.id,
        ).await.unwrap();
        let result = is_local_or_followed(db_client, actor_id).await.unwrap();
        assert_eq!(result, true);
    }
}
