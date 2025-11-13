use crate::database::{DatabaseClient, DatabaseError};

pub async fn delete_tag(
    db_client: &impl DatabaseClient,
    tag_name: &str,
) -> Result<(), DatabaseError> {
    let count = db_client.execute(
        "DELETE FROM tag WHERE tag_name = $1",
        &[&tag_name],
    ).await?;
    if count == 0 {
        return Err(DatabaseError::NotFound("tag"));
    };
    Ok(())
}

pub async fn search_tags(
    db_client: &impl DatabaseClient,
    search_query: &str,
    limit: u16,
    offset: u16,
) -> Result<Vec<String>, DatabaseError> {
    let db_search_query = format!("%{}%", search_query);
    let rows = db_client.query(
        "
        SELECT tag_name
        FROM tag
        WHERE tag_name ILIKE $1
        LIMIT $2 OFFSET $3
        ",
        &[&db_search_query, &i64::from(limit), &i64::from(offset),],
    ).await?;
    let tags: Vec<String> = rows.iter()
        .map(|row| row.try_get("tag_name"))
        .collect::<Result<_, _>>()?;
    Ok(tags)
}

pub async fn find_unused_tags(
    db_client: &impl DatabaseClient,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT tag.tag_name
        FROM tag
        WHERE
            NOT EXISTS (
                SELECT 1
                FROM post_tag
                WHERE post_tag.tag_id = tag.id
            )
        ",
        &[],
    ).await?;
    let tags = rows.iter()
        .map(|row| row.try_get("tag_name"))
        .collect::<Result<_, _>>()?;
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_search_tags() {
        let db_client = &create_test_database().await;
        let results = search_tags(db_client, "test", 1, 0).await.unwrap();
        assert_eq!(results.is_empty(), true);
    }
}
