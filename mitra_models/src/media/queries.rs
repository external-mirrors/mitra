use chrono::Utc;

use crate::background_jobs::{
    queries::enqueue_job,
    types::JobType,
};
use crate::database::{DatabaseClient, DatabaseError};

use super::types::DeletionQueue;

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
            JobType::MediaCleanup,
            &job_data,
            scheduled_for,
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
        WHERE fname NOT IN (
            SELECT file_name FROM media_attachment
            UNION ALL
            SELECT unnest(array_remove(
                ARRAY[
                    avatar ->> 'file_name',
                    banner ->> 'file_name'
                ],
                NULL
            )) AS file_name FROM actor_profile
            UNION ALL
            SELECT image ->> 'file_name' FROM emoji
        )
        ",
        &[&files],
    ).await?;
    let orphaned_files = rows.iter()
        .map(|row| row.try_get("fname"))
        .collect::<Result<_, _>>()?;
    Ok(orphaned_files)
}

async fn find_orphaned_ipfs_objects(
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

pub async fn get_local_files(
    db_client: &impl DatabaseClient,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT
            unnest(array_remove(
                ARRAY[
                    avatar ->> 'file_name',
                    banner ->> 'file_name'
                ],
                NULL
            )) AS file_name
        FROM actor_profile
        WHERE
            user_id IS NOT NULL
            OR portable_user_id IS NOT NULL
        UNION
        SELECT file_name FROM media_attachment
        JOIN actor_profile ON (media_attachment.owner_id = actor_profile.id)
        WHERE
            actor_profile.user_id IS NOT NULL
            OR actor_profile.portable_user_id IS NOT NULL
        UNION
        SELECT image ->> 'file_name' AS file_name
        FROM emoji
        WHERE hostname IS NULL
        ",
        &[],
    ).await?;
    let filenames = rows.iter()
        .map(|row| row.try_get("file_name"))
        .collect::<Result<_, _>>()?;
    Ok(filenames)
}
