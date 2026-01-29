use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::{
    database::{
        query_macro::query,
        DatabaseClient,
        DatabaseError,
    },
    notifications::helpers::create_reaction_notification,
    posts::{
        queries::{
            get_post_author,
            post_subqueries,
            update_reaction_count,
        },
        types::Visibility,
    },
};

use super::types::{
    LikedPost,
    Reaction,
    ReactionData,
    ReactionDeleted,
    ReactionDetailed,
};

pub async fn create_reaction(
    db_client: &mut impl DatabaseClient,
    reaction_data: ReactionData,
) -> Result<Reaction, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let reaction_id = generate_ulid();
    // Reactions to reposts are not allowed
    let maybe_row = transaction.query_opt(
        "
        INSERT INTO post_reaction (
            id,
            author_id,
            post_id,
            content,
            emoji_id,
            visibility,
            activity_id
        )
        SELECT $1, $2, post.id, $4, $5, $6, $7
        FROM (
            SELECT
                CASE WHEN post.repost_of_id IS NULL THEN post.id ELSE NULL
                END AS id
            FROM post WHERE post.id = $3
        ) AS post
        ON CONFLICT DO NOTHING
        RETURNING post_reaction
        ",
        &[
            &reaction_id,
            &reaction_data.author_id,
            &reaction_data.post_id,
            &reaction_data.content,
            &reaction_data.emoji_id,
            &reaction_data.visibility,
            &reaction_data.activity_id,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::AlreadyExists("reaction"))?;
    let reaction: Reaction = row.try_get("post_reaction")?;
    update_reaction_count(&transaction, reaction.post_id, 1).await?;
    let post_author = get_post_author(&transaction, reaction.post_id).await?;
    if post_author.is_local() && post_author.id != reaction.author_id {
        create_reaction_notification(
            &transaction,
            reaction.author_id,
            post_author.id,
            reaction.post_id,
            reaction.id,
        ).await?;
    };
    transaction.commit().await?;
    Ok(reaction)
}

pub async fn get_remote_reaction_by_activity_id(
    db_client: &impl DatabaseClient,
    activity_id: &str,
) -> Result<Reaction, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT post_reaction
        FROM post_reaction
        WHERE activity_id = $1
        ",
        &[&activity_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("reaction"))?;
    let reaction = row.try_get("post_reaction")?;
    Ok(reaction)
}

pub async fn delete_reaction(
    db_client: &mut impl DatabaseClient,
    author_id: Uuid,
    post_id: Uuid,
    maybe_content: Option<&str>,
) -> Result<ReactionDeleted, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        DELETE FROM post_reaction
        WHERE author_id = $1 AND post_id = $2
            AND ($3::text IS NULL AND content IS NULL OR content = $3)
        RETURNING
            post_reaction.id,
            post_reaction.has_deprecated_ap_id,
            post_reaction.visibility
        ",
        &[&author_id, &post_id, &maybe_content],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("reaction"))?;
    let reaction_deleted = ReactionDeleted {
        id: row.try_get("id")?,
        has_deprecated_ap_id: row.try_get("has_deprecated_ap_id")?,
        visibility: row.try_get("visibility")?,
    };
    update_reaction_count(&transaction, post_id, -1).await?;
    transaction.commit().await?;
    Ok(reaction_deleted)
}

pub async fn get_reactions(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    current_user_id: Option<Uuid>,
    max_reaction_id: Option<Uuid>,
    limit: Option<u16>,
) -> Result<Vec<ReactionDetailed>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post_reaction,
            actor_profile AS author,
            emoji
        FROM post_reaction
        JOIN post ON post_reaction.post_id = post.id
        JOIN actor_profile ON post_reaction.author_id = actor_profile.id
        LEFT JOIN emoji ON post_reaction.emoji_id = emoji.id
        WHERE
            post_reaction.post_id = $1
            AND (
                post_reaction.visibility = {visibility_public}
                OR post_reaction.author_id = $2
                OR post.author_id = $2
            )
            AND ($3::uuid IS NULL OR post_reaction.id < $3)
        ORDER BY post_reaction.id DESC
        {limit}
        ",
        visibility_public=i16::from(Visibility::Public),
        limit=limit.map(|n| format!("LIMIT {n}")).unwrap_or("".to_owned()),
    );
    let rows = db_client.query(
        &statement,
        &[
            &post_id,
            &current_user_id,
            &max_reaction_id,
        ],
    ).await?;
    let reactions = rows.iter()
        .map(ReactionDetailed::try_from)
        .collect::<Result<_, _>>()?;
    Ok(reactions)
}

/// Finds posts with reactions among given posts and returns their IDs.
pub(crate) async fn find_reacted_by_user(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<(Uuid, Option<String>)>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post_id, content
        FROM post_reaction
        WHERE author_id = $1 AND post_id = ANY($2)
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let reactions = rows.iter()
        .map(|row| {
            let post_id = row.try_get("post_id")?;
            let content = row.try_get("content")?;
            Ok((post_id, content))
        })
        .collect::<Result<Vec<_>, DatabaseError>>()?;
    Ok(reactions)
}

pub async fn get_liked_posts(
    db_client: &impl DatabaseClient,
    author_id: Uuid,
    max_reaction_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<LikedPost>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post_reaction.id,
            post,
            actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        JOIN post_reaction ON post_reaction.post_id = post.id
        WHERE
            post_reaction.author_id = $author_id
            AND post_reaction.content IS NULL
            AND (
                $max_reaction_id::uuid IS NULL
                OR post_reaction.id < $max_reaction_id
            )
        ORDER BY post_reaction.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
    );
    let limit = i64::from(limit);
    let query = query!(
        &statement,
        author_id=author_id,
        max_reaction_id=max_reaction_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let liked_posts = rows.iter()
        .map(LikedPost::try_from)
        .collect::<Result<_, _>>()?;
    Ok(liked_posts)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        posts::{
            queries::{create_post, get_post_by_id},
            types::{PostCreateData, Visibility},
        },
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_reaction() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let post_data = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, user_2.id, post_data).await.unwrap();
        let content = "❤️";
        let reaction_data = ReactionData {
            author_id: user_1.id,
            post_id: post.id,
            content: Some(content.to_string()),
            emoji_id: None,
            visibility: Visibility::Direct,
            activity_id: None,
        };
        let reaction = create_reaction(db_client, reaction_data).await.unwrap();

        assert_eq!(reaction.author_id, user_1.id);
        assert_eq!(reaction.post_id, post.id);
        assert_eq!(reaction.content.unwrap(), content);
        assert_eq!(reaction.emoji_id.is_none(), true);
        assert_eq!(reaction.visibility, Visibility::Direct);
        assert_eq!(reaction.activity_id.is_none(), true);

        let post = get_post_by_id(db_client, post.id).await.unwrap();
        assert_eq!(post.reactions.len(), 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_reaction_uniqueness() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let post_data = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, user_2.id, post_data).await.unwrap();
        let reaction_data_1 = ReactionData {
            author_id: user_1.id,
            post_id: post.id,
            content: None,
            emoji_id: None,
            visibility: Visibility::Direct,
            activity_id: None,
        };
        create_reaction(db_client, reaction_data_1).await.unwrap();
        let reaction_data_2 = ReactionData {
            author_id: user_1.id,
            post_id: post.id,
            content: Some("❤️".to_string()),
            emoji_id: None,
            visibility: Visibility::Direct,
            activity_id: None,
        };
        create_reaction(db_client, reaction_data_2.clone()).await.unwrap();
        let error = create_reaction(db_client, reaction_data_2).await.err().unwrap();
        assert_eq!(error.to_string(), "reaction already exists");
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_reaction() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let post_data = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, user_2.id, post_data).await.unwrap();
        let reaction_data = ReactionData {
            author_id: user_1.id,
            post_id: post.id,
            content: None,
            emoji_id: None,
            visibility: Visibility::Direct,
            activity_id: None,
        };
        let reaction = create_reaction(db_client, reaction_data).await.unwrap();
        let reaction_deleted = delete_reaction(
            db_client,
            user_1.id,
            post.id,
            None,
        ).await.unwrap();
        assert_eq!(reaction_deleted.id, reaction.id);
        assert_eq!(reaction_deleted.has_deprecated_ap_id, false);
        assert_eq!(reaction_deleted.visibility, Visibility::Direct);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_reactions() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let user_3 = create_test_user(db_client, "test3").await;
        let post_data = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, user_1.id, post_data).await.unwrap();
        let reaction_data_1 = ReactionData {
            author_id: user_2.id,
            post_id: post.id,
            content: None,
            emoji_id: None,
            visibility: Visibility::Direct,
            activity_id: None,
        };
        let _reaction_1 = create_reaction(db_client, reaction_data_1).await.unwrap();
        let reaction_data_2 = ReactionData {
            author_id: user_3.id,
            post_id: post.id,
            content: None,
            emoji_id: None,
            visibility: Visibility::Public,
            activity_id: None,
        };
        let reaction_2 = create_reaction(db_client, reaction_data_2).await.unwrap();
        let reactions = get_reactions(
            db_client,
            post.id,
            None, // guest
            None,
            None,
        ).await.unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].id, reaction_2.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_liked_posts() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let user_2 = create_test_user(db_client, "test2").await;
        let post_data = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, user_2.id, post_data).await.unwrap();
        let reaction_data = ReactionData {
            author_id: user_1.id,
            post_id: post.id,
            content: None,
            emoji_id: None,
            visibility: Visibility::Direct,
            activity_id: None,
        };
        let reaction = create_reaction(db_client, reaction_data).await.unwrap();
        let liked_posts = get_liked_posts(
            db_client,
            user_1.id,
            None,
            20,
        ).await.unwrap();
        assert_eq!(liked_posts.len(), 1);
        assert_eq!(liked_posts[0].reaction_id, reaction.id);
    }
}
