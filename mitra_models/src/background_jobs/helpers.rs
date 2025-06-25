use uuid::Uuid;

use crate::database::{
    get_database_client,
    DatabaseConnectionPool,
    DatabaseError,
};

use super::{
    queries::{delete_job_from_queue, get_job_batch},
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

pub async fn delete_job_from_queue_with_pool(
    db_pool: &DatabaseConnectionPool,
    job_id: Uuid,
) -> Result<(), DatabaseError> {
    let db_client = &**get_database_client(db_pool).await?;
    delete_job_from_queue(db_client, job_id).await
}
