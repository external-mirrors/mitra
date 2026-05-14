use crate::database::{DatabaseClient, DatabaseError};

pub async fn refresh_latest_post_view(
    db_client: &impl DatabaseClient,
) -> Result<(), DatabaseError> {
    db_client.execute("REFRESH MATERIALIZED VIEW latest_post", &[]).await?;
    Ok(())
}
