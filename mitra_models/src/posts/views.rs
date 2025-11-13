use crate::database::{
    get_database_client,
    DatabaseConnectionPool,
    DatabaseError,
};

pub async fn refresh_latest_post_view(
    db_pool: &DatabaseConnectionPool,
) -> Result<(), DatabaseError> {
    let db_client = &**get_database_client(db_pool).await?;
    db_client.execute("REFRESH MATERIALIZED VIEW latest_post", &[]).await?;
    Ok(())
}
