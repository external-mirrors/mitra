use super::{DatabaseClient, DatabaseError, DatabaseTypeError};

pub async fn get_postgres_version(
    db_client: &impl  DatabaseClient,
) -> Result<u32, DatabaseError> {
    let row = db_client.query_one("SHOW server_version_num", &[]).await?;
    let version_str: String = row.try_get("server_version_num")?;
    let version: u32 = version_str.parse().map_err(|_| DatabaseTypeError)?;
    Ok(version)
}
