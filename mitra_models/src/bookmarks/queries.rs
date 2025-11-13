use uuid::Uuid;

use crate::{
    database::{
        catch_unique_violation,
        query_macro::query,
        DatabaseClient,
        DatabaseError,
    },
    posts::{
        queries::post_subqueries,
    },
};

use super::types::BookmarkedPost;

pub async fn create_bookmark(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    post_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO bookmark (owner_id, post_id)
        VALUES ($1, $2)
        ",
        &[&owner_id, &post_id],
    ).await.map_err(catch_unique_violation("bookmark"))?;
    Ok(())
}

pub async fn delete_bookmark(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    post_id: Uuid,
) -> Result<(), DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM bookmark
        WHERE owner_id = $1 AND post_id = $2
        ",
        &[&owner_id, &post_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("bookmark"));
    };
    Ok(())
}

pub async fn get_bookmarked_posts(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    max_bookmark_id: Option<i32>,
    limit: u16,
) -> Result<Vec<BookmarkedPost>, DatabaseError> {
    // No visibility check because it was done when bookmark was created
    let statement = format!(
        "
        SELECT
            bookmark.id,
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        JOIN bookmark ON bookmark.post_id = post.id
        WHERE
            bookmark.owner_id = $owner_id
            AND (
                $max_bookmark_id::integer IS NULL
                OR bookmark.id < $max_bookmark_id
            )
        ORDER BY bookmark.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
    );
    let limit = i64::from(limit);
    let query = query!(
        &statement,
        owner_id=owner_id,
        max_bookmark_id=max_bookmark_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let bookmarks = rows.iter()
        .map(BookmarkedPost::try_from)
        .collect::<Result<_, _>>()?;
    Ok(bookmarks)
}

pub(crate) async fn find_bookmarked_by_user(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post_id
        FROM bookmark
        WHERE owner_id = $1 AND post_id = ANY($2)
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let bookmarked = rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    Ok(bookmarked)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        posts::test_utils::create_test_local_post,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_and_delete_bookmark() {
        let db_client = &mut create_test_database().await;
        let viewer = create_test_user(db_client, "viewer").await;
        let author = create_test_user(db_client, "author").await;
        let post = create_test_local_post(
            db_client,
            author.id,
            "test post",
        ).await;

        create_bookmark(db_client, viewer.id, post.id).await.unwrap();
        let bookmarks = get_bookmarked_posts(
            db_client,
            viewer.id,
            None,
            5
        ).await.unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].post.id, post.id);

        delete_bookmark(db_client, viewer.id, post.id).await.unwrap();
        let bookmarks = get_bookmarked_posts(
            db_client,
            viewer.id,
            None,
            5
        ).await.unwrap();
        assert_eq!(bookmarks.len(), 0);
    }
}
