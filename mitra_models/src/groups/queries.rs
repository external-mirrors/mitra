use uuid::Uuid;

use crate::{
    accounts::{
        types::{
            AutomatedAccountDetailed,
            AutomatedAccountType,
        },
    },
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
    profiles::types::{ActorType, DbActorProfile},
    relationships::types::RelationshipType,
};

use super::types::GroupFilter;

pub async fn get_related_groups(
    db_client: &impl DatabaseClient,
    account_id: Uuid,
    filter: GroupFilter,
    offset: u16,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let relationship_type = match filter {
        GroupFilter::Following => RelationshipType::Follow,
        GroupFilter::Moderating => RelationshipType::GroupAdmin,
    };
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
            actor_profile.actor_type = $1
            AND relationship.source_id = $2
            AND relationship.relationship_type = $3
        ORDER BY latest_post.created_at DESC NULLS LAST
        LIMIT $4
        OFFSET $5
        ",
        &[
            &ActorType::Group,
            &account_id,
            &relationship_type,
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

pub async fn get_orphaned_local_groups(
    db_client: &impl DatabaseClient,
) -> Result<Vec<AutomatedAccountDetailed>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT automated_account, actor_profile
        FROM automated_account
        JOIN actor_profile USING (id)
        WHERE
            automated_account.account_type = $1
            AND NOT EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    relationship.target_id = automated_account.id
                    AND relationship.relationship_type = $2
            )
        ",
        &[
            &AutomatedAccountType::Group,
            &RelationshipType::GroupAdmin,
        ],
    ).await?;
    let groups = rows.iter()
        .map(AutomatedAccountDetailed::try_from)
        .collect::<Result<_, _>>()?;
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        accounts::test_utils::create_test_user,
        activitypub::constants::AP_PUBLIC,
        database::test_utils::create_test_database,
        groups::{
            helpers::{
                create_group,
                join_private_group,
            },
            test_utils::create_test_remote_group,
            types::GroupCreateData,
        },
        posts::{
            queries::create_post,
            types::{PostCreateData, PostContext, Visibility},
        },
        profiles::{
            queries::{create_profile, delete_profile},
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
            group_data.actor_type = ActorType::Group;
            group_data
        };
        let group = create_profile(db_client, group_data).await.unwrap();
        follow(db_client, account.id, group.id).await.unwrap();
        let groups = get_related_groups(
            db_client,
            account.id,
            GroupFilter::Following,
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
        let group = create_test_remote_group(
            db_client,
            "group",
            "groups.example",
            "https://groups.example/123",
            false, // public group
        ).await;
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

    #[tokio::test]
    #[serial]
    async fn test_get_group_timeline_private_group() {
        let db_client = &mut create_test_database().await;
        let group = create_test_remote_group(
            db_client,
            "group",
            "groups.example",
            "https://groups.example/123",
            true,
        ).await;
        assert!(group.is_private_group());
        let author = create_test_remote_profile(
            db_client,
            "author",
            "social.example",
            "https://social.example/users/1",
        ).await;
        join_private_group(db_client, author.id, group.id).await.unwrap();
        let post_data = PostCreateData {
            context: PostContext::Top {
                group_id: Some(group.id),
                object_id: Some("https://social.example/contexts/123".to_owned()),
                audience: Some("https://groups.example/123/followers".to_owned()),
            },
            visibility: Visibility::Group,
            object_id: Some("https://social.example/posts/123".to_owned()),
            ..PostCreateData::for_test()
        };
        let post = create_post(
            db_client,
            author.id,
            post_data,
        ).await.unwrap();

        let account = create_test_user(db_client, "user").await;
        join_private_group(db_client, account.id, group.id).await.unwrap();
        let posts = get_group_timeline(
            db_client,
            group.id,
            account.id,
            None,
            20,
        ).await.unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, post.id);

        let non_member_account = create_test_user(db_client, "other").await;
        let posts = get_group_timeline(
            db_client,
            group.id,
            non_member_account.id,
            None,
            20,
        ).await.unwrap();
        assert_eq!(posts.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_orphaned_local_groups() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "user").await;
        let group_data = GroupCreateData::for_test("group");
        let group = create_group(db_client, user.id, group_data).await.unwrap();
        let orphaned = get_orphaned_local_groups(db_client).await.unwrap();
        assert_eq!(orphaned.len(), 0);
        delete_profile(db_client, user.id).await.unwrap();
        let orphaned = get_orphaned_local_groups(db_client).await.unwrap();
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0].id, group.id);
    }
}
