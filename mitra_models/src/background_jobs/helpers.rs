use crate::database::{
    get_database_client,
    DatabaseConnectionPool,
    DatabaseError,
};

use super::{
    queries::get_job_batch,
    types::{DbBackgroundJob, JobType},
};

pub async fn get_job_batch_with_pool(
    db_pool: &DatabaseConnectionPool,
    job_type: JobType,
    batch_size: u32,
    job_timeout: u32,
) -> Result<Vec<DbBackgroundJob>, DatabaseError> {
    let db_client = &**get_database_client(db_pool).await?;
    get_job_batch(db_client, job_type, batch_size, job_timeout).await
}
