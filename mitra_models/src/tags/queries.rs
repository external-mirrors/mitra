use crate::database::{DatabaseClient, DatabaseError};

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
