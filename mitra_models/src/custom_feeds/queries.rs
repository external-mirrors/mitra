use uuid::Uuid;

use crate::{
    database::{
        catch_unique_violation,
        DatabaseClient,
        DatabaseError,
    },
    profiles::types::DbActorProfile,
};

use super::types::CustomFeed;

pub async fn create_custom_feed(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    feed_name: &str,
) -> Result<CustomFeed, DatabaseError> {
    let row = db_client.query_one(
        "
        INSERT INTO custom_feed (owner_id, feed_name)
        VALUES ($1, $2)
        RETURNING custom_feed
        ",
        &[&owner_id, &feed_name],
    ).await.map_err(catch_unique_violation("custom feed"))?;
    let feed = row.try_get("custom_feed")?;
    Ok(feed)
}

pub async fn update_custom_feed(
    db_client: &impl DatabaseClient,
    feed_id: i32,
    owner_id: Uuid,
    feed_name: &str,
) -> Result<CustomFeed, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE custom_feed
        SET feed_name = $3
        WHERE id = $1 AND owner_id = $2
        RETURNING custom_feed
        ",
        &[&feed_id, &owner_id, &feed_name],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("custom feed"))?;
    let feed = row.try_get("custom_feed")?;
    Ok(feed)
}

pub async fn delete_custom_feed(
    db_client: &impl DatabaseClient,
    feed_id: i32,
    owner_id: Uuid,
) -> Result<(), DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM custom_feed
        WHERE id = $1 AND owner_id = $2
        ",
        &[&feed_id, &owner_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("custom feed"));
    };
    Ok(())
}

pub async fn get_custom_feed(
    db_client: &impl DatabaseClient,
    feed_id: i32,
    owner_id: Uuid,
) -> Result<CustomFeed, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT custom_feed
        FROM custom_feed
        WHERE id = $1 AND owner_id = $2
        ",
        &[&feed_id, &owner_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("custom feed"))?;
    let feed = row.try_get("custom_feed")?;
    Ok(feed)
}

pub async fn get_custom_feeds(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
) -> Result<Vec<CustomFeed>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT custom_feed
        FROM custom_feed
        WHERE owner_id = $1
        ORDER BY id DESC
        ",
        &[&owner_id],
    ).await?;
    let feeds = rows.iter()
        .map(|row| row.try_get("custom_feed"))
        .collect::<Result<_, _>>()?;
    Ok(feeds)
}

pub async fn add_custom_feed_sources(
    db_client: &impl DatabaseClient,
    feed_id: i32,
    profile_ids: &[Uuid],
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO custom_feed_source (feed_id, source_id)
        SELECT $1, unnest($2::uuid[])
        ",
        &[&feed_id, &profile_ids],
    ).await.map_err(catch_unique_violation("custom feed source"))?;
    Ok(())
}

pub async fn remove_custom_feed_sources(
    db_client: &impl DatabaseClient,
    feed_id: i32,
    profile_ids: &[Uuid],
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        DELETE FROM custom_feed_source
        WHERE feed_id = $1 AND source_id = ANY($2)
        ",
        &[&feed_id, &profile_ids],
    ).await?;
    Ok(())
}

pub async fn get_custom_feed_sources(
    db_client: &impl DatabaseClient,
    feed_id: i32,
    max_source_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM custom_feed_source
        JOIN actor_profile ON custom_feed_source.source_id = actor_profile.id
        WHERE
            custom_feed_source.feed_id = $1
            AND ($2::uuid IS NULL OR actor_profile.id < $2)
        ORDER BY actor_profile.id DESC
        LIMIT $3
        ",
        &[&feed_id, &max_source_id, &i64::from(limit)],
    ).await?;
    let sources = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(sources)
}

pub async fn get_custom_feeds_by_source(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    source_id: Uuid,
) -> Result<Vec<CustomFeed>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT custom_feed
        FROM custom_feed
        JOIN custom_feed_source ON custom_feed_source.feed_id = custom_feed.id
        WHERE custom_feed.owner_id = $1
            AND custom_feed_source.source_id = $2
        ",
        &[&owner_id, &source_id],
    ).await?;
    let feeds = rows.iter()
        .map(|row| row.try_get("custom_feed"))
        .collect::<Result<_, _>>()?;
    Ok(feeds)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_custom_feed() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "user").await;
        let feed_name = "test feed";
        let feed = create_custom_feed(
            db_client,
            user.id,
            feed_name,
        ).await.unwrap();
        assert_eq!(feed.owner_id, user.id);
        assert_eq!(feed.feed_name, feed_name);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_custom_feed() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "user").await;
        let feed_name = "test feed";
        let feed = create_custom_feed(
            db_client,
            user.id,
            feed_name,
        ).await.unwrap();
        let updated_feed = update_custom_feed(
            db_client,
            feed.id,
            user.id,
            "My custom feed",
        ).await.unwrap();
        assert_eq!(updated_feed.feed_name, "My custom feed");
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_custom_feed() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "user").await;
        let feed_name = "test feed";
        let feed = create_custom_feed(
            db_client,
            user.id,
            feed_name,
        ).await.unwrap();
        let result = delete_custom_feed(
            db_client,
            feed.id,
            user.id,
        ).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_add_remove_custom_feed_sources() {
        let db_client = &mut create_test_database().await;
        let viewer = create_test_user(db_client, "viewer").await;
        let author_1 = create_test_user(db_client, "author_1").await;
        let author_2 = create_test_user(db_client, "author_2").await;
        let feed = create_custom_feed(
            db_client,
            viewer.id,
            "test",
        ).await.unwrap();
        add_custom_feed_sources(
            db_client,
            feed.id,
            &[author_1.id, author_2.id],
        ).await.unwrap();
        let error = add_custom_feed_sources(
            db_client,
            feed.id,
            &[author_2.id],
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "custom feed source already exists");

        let sources = get_custom_feed_sources(
            db_client,
            feed.id,
            None,
            5,
        ).await.unwrap();
        assert_eq!(sources.len(), 2);
        let feeds = get_custom_feeds_by_source(
            db_client,
            viewer.id,
            author_1.id,
        ).await.unwrap();
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].id, feed.id);

        remove_custom_feed_sources(
            db_client,
            feed.id,
            &[author_2.id],
        ).await.unwrap();
        let sources = get_custom_feed_sources(
            db_client,
            feed.id,
            None,
            5,
        ).await.unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, author_1.id);
    }
}
