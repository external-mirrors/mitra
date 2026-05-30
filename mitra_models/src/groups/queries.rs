use uuid::Uuid;

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
    profiles::types::DbActorProfile,
    relationships::types::RelationshipType,
};

pub async fn get_followed_groups(
    db_client: &impl DatabaseClient,
    account_id: Uuid,
    offset: u16,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    // TODO: include local groups
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
            ON actor_profile.id = relationship.target_id
        LEFT JOIN LATERAL (
            SELECT post.created_at
            FROM post
            WHERE post.group_id = actor_profile.id
            ORDER BY post.created_at DESC
            LIMIT 1
        ) AS latest_post ON TRUE
        WHERE
            actor_profile.actor_json ->> 'type' = 'Group'
            AND relationship.source_id = $1
            AND relationship.relationship_type = $2
        ORDER BY latest_post.created_at DESC NULLS LAST
        LIMIT $3
        OFFSET $4
        ",
        &[
            &account_id,
            &RelationshipType::Follow,
            &i64::from(limit),
            &i64::from(offset),
        ],
    ).await?;
    let groups = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        profiles::{
            queries::create_profile,
            types::ProfileCreateData,
        },
        relationships::queries::follow,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_get_followed_groups() {
        let db_client = &mut create_test_database().await;
        let account = create_test_user(db_client, "user").await;
        let group_data = {
            let mut group_data = ProfileCreateData::remote_for_test(
                "group",
                "groups.example",
                "https://groups.example/123",
            );
            let actor_data = group_data.actor_json.as_mut().unwrap();
            actor_data.object_type = "Group".to_owned();
            group_data
        };
        let group = create_profile(db_client, group_data).await.unwrap();
        follow(db_client, account.id, group.id).await.unwrap();
        let groups = get_followed_groups(
            db_client,
            account.id,
            0,
            20,
        ).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, group.id);
    }
}
