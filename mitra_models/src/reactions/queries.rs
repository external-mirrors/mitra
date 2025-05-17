use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::database::{
    DatabaseClient,
    DatabaseError,
};
use crate::notifications::helpers::create_reaction_notification;
use crate::posts::queries::{
    update_reaction_count,
    get_post_author,
};

use super::types::{DbReaction, ReactionData};

pub async fn create_reaction(
    db_client: &mut impl DatabaseClient,
    reaction_data: ReactionData,
) -> Result<DbReaction, DatabaseError> {
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
            activity_id
        )
        SELECT $1, $2, post.id, $4, $5, $6
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
            &reaction_data.activity_id,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::AlreadyExists("reaction"))?;
    let reaction: DbReaction = row.try_get("post_reaction")?;
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
) -> Result<DbReaction, DatabaseError> {
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
) -> Result<(Uuid, bool), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        DELETE FROM post_reaction
        WHERE author_id = $1 AND post_id = $2
            AND ($3::text IS NULL AND content IS NULL OR content = $3)
        RETURNING
            post_reaction.id,
            post_reaction.has_deprecated_ap_id
        ",
        &[&author_id, &post_id, &maybe_content],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("reaction"))?;
    let reaction_id = row.try_get("id")?;
    let reaction_has_deprecated_ap_id = row.try_get("has_deprecated_ap_id")?;
    update_reaction_count(&transaction, post_id, -1).await?;
    transaction.commit().await?;
    Ok((reaction_id, reaction_has_deprecated_ap_id))
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

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        posts::{
            queries::{create_post, get_post_by_id},
            types::PostCreateData,
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
            activity_id: None,
        };
        let reaction = create_reaction(db_client, reaction_data).await.unwrap();

        assert_eq!(reaction.author_id, user_1.id);
        assert_eq!(reaction.post_id, post.id);
        assert_eq!(reaction.content.unwrap(), content);
        assert_eq!(reaction.emoji_id.is_none(), true);
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
            activity_id: None,
        };
        create_reaction(db_client, reaction_data_1).await.unwrap();
        let reaction_data_2 = ReactionData {
            author_id: user_1.id,
            post_id: post.id,
            content: Some("❤️".to_string()),
            emoji_id: None,
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
            activity_id: None,
        };
        let reaction = create_reaction(db_client, reaction_data).await.unwrap();
        let (reaction_id, reaction_has_deprecated_ap_id) = delete_reaction(
            db_client,
            user_1.id,
            post.id,
            None,
        ).await.unwrap();
        assert_eq!(reaction_id, reaction.id);
        assert_eq!(reaction_has_deprecated_ap_id, false);
    }
}
