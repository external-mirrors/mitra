use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::background_jobs::{
    queries::enqueue_job,
    types::JobType,
};
use crate::database::{DatabaseClient, DatabaseError};

#[derive(Deserialize, Serialize)]
pub struct DeletionQueue {
    pub files: Vec<String>,
    pub ipfs_objects: Vec<String>,
}

impl DeletionQueue {
    pub async fn into_job(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        let job_data = serde_json::to_value(self)
            .expect("cleanup data should be serializable");
        let scheduled_for = Utc::now(); // run immediately
        enqueue_job(
            db_client,
            &JobType::MediaCleanup,
            &job_data,
            &scheduled_for,
        ).await
    }

    /// Find and remove non-orphaned objects
    pub async fn filter_objects(
        &mut self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        self.files = find_orphaned_files(
            db_client,
            self.files.clone(),
        ).await?;
        self.ipfs_objects = find_orphaned_ipfs_objects(
            db_client,
            self.ipfs_objects.clone(),
        ).await?;
        Ok(())
    }
}

pub async fn find_orphaned_files(
    db_client: &impl DatabaseClient,
    files: Vec<String>,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT DISTINCT fname
        FROM unnest($1::text[]) AS fname
        WHERE
            NOT EXISTS (
                SELECT 1 FROM media_attachment WHERE file_name = fname
            )
            AND NOT EXISTS (
                SELECT 1 FROM actor_profile
                WHERE avatar ->> 'file_name' = fname
                    OR banner ->> 'file_name' = fname
            )
            AND NOT EXISTS (
                SELECT 1 FROM emoji
                WHERE image ->> 'file_name' = fname
            )
        ",
        &[&files],
    ).await?;
    let orphaned_files = rows.iter()
        .map(|row| row.try_get("fname"))
        .collect::<Result<_, _>>()?;
    Ok(orphaned_files)
}

pub(super) async fn find_orphaned_ipfs_objects(
    db_client: &impl DatabaseClient,
    ipfs_objects: Vec<String>,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT DISTINCT cid
        FROM unnest($1::text[]) AS cid
        WHERE
            NOT EXISTS (
                SELECT 1 FROM media_attachment WHERE ipfs_cid = cid
            )
            AND NOT EXISTS (
                SELECT 1 FROM post WHERE ipfs_cid = cid
            )
        ",
        &[&ipfs_objects],
    ).await?;
    let orphaned_ipfs_objects = rows.iter()
        .map(|row| row.try_get("cid"))
        .collect::<Result<_, _>>()?;
    Ok(orphaned_ipfs_objects)
}
