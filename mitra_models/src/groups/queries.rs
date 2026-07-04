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
    relationships::{
        queries::create_relationship,
        types::RelationshipType,
    },
};

use super::types::{GroupCreateData, GroupFilter};

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
    create_relationship(
        &transaction,
        owner_id,
        account.id,
        RelationshipType::GroupAdmin,
    ).await?;
    transaction.commit().await?;
    Ok(account)
}

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

#[cfg(test)]
mod tests {
    use apx_core::crypto::{
        eddsa::generate_weak_ed25519_key,
        rsa::generate_weak_rsa_key,
    };
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
        relationships::{
            queries::{follow, has_relationship},
            types::RelationshipType,
        },
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
