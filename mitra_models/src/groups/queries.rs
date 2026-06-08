use uuid::Uuid;

use crate::{
    database::{
        query_macro::query,
        DatabaseClient,
        DatabaseError,
    },
    posts::{
        queries::{
            build_mute_filter,
            build_visibility_filter,
            post_subqueries,
        },
        types::PostDetailed,
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

pub async fn get_group_timeline(
    db_client: &impl DatabaseClient,
    group_id: Uuid,
    current_account_id: Uuid,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<PostDetailed>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post,
            actor_profile AS post_author,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            post.group_id = $group_id
            AND post.in_reply_to_id IS NULL
            AND {visibility_filter}
            AND {mute_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
        visibility_filter=build_visibility_filter(),
        mute_filter=build_mute_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        group_id=group_id,
        current_user_id=current_account_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts = rows.iter()
        .map(PostDetailed::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        accounts::test_utils::create_test_user,
        activitypub::constants::AP_PUBLIC,
        database::test_utils::create_test_database,
        posts::{
            queries::create_post,
            types::{PostCreateData, PostContext},
        },
        profiles::{
            queries::create_profile,
            test_utils::create_test_remote_profile,
            types::ProfileCreateData,
        },
        relationships::queries::follow,
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

    #[tokio::test]
    #[serial]
    async fn test_get_group_timeline() {
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
        let author = create_test_remote_profile(
            db_client,
            "author",
            "social.example",
            "https://social.example/users/1",
        ).await;
        let post_data = PostCreateData {
            context: PostContext::Top {
                group_id: Some(group.id),
                object_id: Some("https://social.example/contexts/123".to_owned()),
                audience: Some(AP_PUBLIC.to_owned()),
            },
            object_id: Some("https://social.example/posts/123".to_owned()),
            ..PostCreateData::for_test()
        };
        let post =
            create_post(db_client, author.id, post_data).await.unwrap();
        let posts = get_group_timeline(
            db_client,
            group.id,
            account.id,
            None,
            20,
        ).await.unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, post.id);
    }
}
