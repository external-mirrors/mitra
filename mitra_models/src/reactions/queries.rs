use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::database::{
    catch_unique_violation,
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
        SELECT $1, $2, $3, $4, $5, $6
        WHERE NOT EXISTS (
            SELECT 1 FROM post
            WHERE post.id = $3 AND post.repost_of_id IS NOT NULL
        )
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
    ).await.map_err(catch_unique_violation("reaction"))?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
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

pub async fn get_reaction_by_remote_activity_id(
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
) -> Result<Uuid, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        DELETE FROM post_reaction
        WHERE author_id = $1 AND post_id = $2
        RETURNING post_reaction.id
        ",
        &[&author_id, &post_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("reaction"))?;
    let reaction_id = row.try_get("id")?;
    update_reaction_count(&transaction, post_id, -1).await?;
    transaction.commit().await?;
    Ok(reaction_id)
}

/// Finds favourites among given posts and returns their IDs
pub async fn find_favourited_by_user(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post_id
        FROM post_reaction
        WHERE author_id = $1 AND post_id = ANY($2)
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let favourites: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    Ok(favourites)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::posts::{
        queries::create_post,
        types::PostCreateData,
    };
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_reaction() {
        let db_client = &mut create_test_database().await;
        let user_data_1 = UserCreateData {
            username: "test1".to_string(),
            password_hash: Some("test1".to_string()),
            ..Default::default()
        };
        let user_1 = create_user(db_client, user_data_1).await.unwrap();
        let user_data_2 = UserCreateData {
            username: "test2".to_string(),
            password_hash: Some("test2".to_string()),
            ..Default::default()
        };
        let user_2 = create_user(db_client, user_data_2).await.unwrap();
        let post_data = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, &user_2.id, post_data).await.unwrap();
        let emoji = "❤️";
        let reaction_data = ReactionData {
            author_id: user_1.id,
            post_id: post.id,
            content: Some(emoji.to_string()),
            emoji_id: None,
            activity_id: None,
        };
        let reaction = create_reaction(db_client, reaction_data).await.unwrap();

        assert_eq!(reaction.author_id, user_1.id);
        assert_eq!(reaction.post_id, post.id);
        assert_eq!(reaction.content.unwrap(), emoji);
        assert_eq!(reaction.emoji_id.is_none(), true);
        assert_eq!(reaction.activity_id.is_none(), true);
    }
}
