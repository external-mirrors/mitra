use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use crate::database::{DatabaseClient, DatabaseError};

use super::types::DbMediaAttachment;

pub async fn create_attachment(
    db_client: &impl DatabaseClient,
    owner_id: &Uuid,
    file_name: String,
    file_size: usize,
    media_type: String,
    description: Option<&str>,
) -> Result<DbMediaAttachment, DatabaseError> {
    let attachment_id = generate_ulid();
    let file_size: i32 = file_size.try_into()
        .expect("value should be within bounds");
    let inserted_row = db_client.query_one(
        "
        INSERT INTO media_attachment (
            id,
            owner_id,
            file_name,
            file_size,
            media_type,
            description
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING media_attachment
        ",
        &[
            &attachment_id,
            &owner_id,
            &file_name,
            &file_size,
            &media_type,
            &description,
        ],
    ).await?;
    let db_attachment: DbMediaAttachment = inserted_row.try_get("media_attachment")?;
    Ok(db_attachment)
}

pub async fn get_attachment(
    db_client: &impl DatabaseClient,
    owner_id: &Uuid,
    attachment_id: &Uuid,
) -> Result<DbMediaAttachment, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT media_attachment
        FROM media_attachment
        WHERE owner_id = $1 AND id = $2
        ",
        &[&owner_id, &attachment_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("attachment"))?;
    let db_attachment = row.try_get("media_attachment")?;
    Ok(db_attachment)
}

pub async fn update_attachment(
    db_client: &impl DatabaseClient,
    owner_id: &Uuid,
    attachment_id: &Uuid,
    description: Option<&str>,
) -> Result<DbMediaAttachment, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE media_attachment
        SET description = $1
        WHERE owner_id = $2 AND id = $3
        RETURNING media_attachment
        ",
        &[&description, &owner_id, &attachment_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("attachment"))?;
    let db_attachment = row.try_get("media_attachment")?;
    Ok(db_attachment)
}

pub async fn set_attachment_ipfs_cid(
    db_client: &impl DatabaseClient,
    attachment_id: &Uuid,
    ipfs_cid: &str,
) -> Result<DbMediaAttachment, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE media_attachment
        SET ipfs_cid = $1
        WHERE id = $2 AND ipfs_cid IS NULL
        RETURNING media_attachment
        ",
        &[&ipfs_cid, &attachment_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("attachment"))?;
    let db_attachment = row.try_get("media_attachment")?;
    Ok(db_attachment)
}

pub async fn delete_unused_attachments(
    db_client: &impl DatabaseClient,
    created_before: &DateTime<Utc>,
) -> Result<DeletionQueue, DatabaseError> {
    let rows = db_client.query(
        "
        DELETE FROM media_attachment
        WHERE post_id IS NULL AND created_at < $1
        RETURNING file_name, ipfs_cid
        ",
        &[&created_before],
    ).await?;
    let mut files = vec![];
    let mut ipfs_objects = vec![];
    for row in rows {
        let file_name = row.try_get("file_name")?;
        files.push(file_name);
        if let Some(ipfs_cid) = row.try_get("ipfs_cid")? {
            ipfs_objects.push(ipfs_cid);
        };
    };
    let orphaned_files = find_orphaned_files(db_client, files).await?;
    let orphaned_ipfs_objects = find_orphaned_ipfs_objects(db_client, ipfs_objects).await?;
    Ok(DeletionQueue {
        files: orphaned_files,
        ipfs_objects: orphaned_ipfs_objects,
    })
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::profiles::{
        queries::create_profile,
        types::ProfileCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_attachment() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let file_name = "test.jpg";
        let file_size = 10000;
        let media_type = "image/png";
        let description = "test";
        let attachment = create_attachment(
            db_client,
            &profile.id,
            file_name.to_string(),
            file_size,
            media_type.to_string(),
            Some(description),
        ).await.unwrap();
        assert_eq!(attachment.owner_id, profile.id);
        assert_eq!(attachment.file_name, file_name);
        assert_eq!(attachment.file_size.unwrap(), file_size as i32);
        assert_eq!(attachment.media_type.unwrap(), media_type);
        assert_eq!(attachment.description.unwrap(), description);
        assert_eq!(attachment.ipfs_cid.is_none(), true);
        assert_eq!(attachment.post_id.is_none(), true);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_attachment() {
        let db_client = &mut create_test_database().await;
        let profile_data_1 = ProfileCreateData {
            username: "test1".to_string(),
            ..Default::default()
        };
        let profile_1 =
            create_profile(db_client, profile_data_1).await.unwrap();
        let profile_data_2 = ProfileCreateData {
            username: "test2".to_string(),
            ..Default::default()
        };
        let profile_2 =
            create_profile(db_client, profile_data_2).await.unwrap();
        let file_name = "test.jpg";
        let file_size = 10000;
        let media_type = "image/png";
        let DbMediaAttachment { id: attachment_id, .. } = create_attachment(
            db_client,
            &profile_1.id,
            file_name.to_string(),
            file_size,
            media_type.to_string(),
            None,
        ).await.unwrap();

        let attachment = get_attachment(
            db_client,
            &profile_1.id,
            &attachment_id,
        ).await.unwrap();
        assert_eq!(attachment.file_name, file_name);

        let error = get_attachment(
            db_client,
            &profile_2.id,
            &attachment_id,
        ).await.err().unwrap();
        assert!(matches!(error, DatabaseError::NotFound(_)));
    }

    #[tokio::test]
    #[serial]
    async fn test_update_attachment_remove_description() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test1".to_string(),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let file_name = "test.jpg";
        let file_size = 10000;
        let media_type = "image/png";
        let description = "test image";
        let attachment = create_attachment(
            db_client,
            &profile.id,
            file_name.to_string(),
            file_size,
            media_type.to_string(),
            Some(description),
        ).await.unwrap();
        assert_eq!(attachment.description.unwrap(), description);

        let attachment_updated = update_attachment(
            db_client,
            &profile.id,
            &attachment.id,
            None,
        ).await.unwrap();
        assert_eq!(attachment_updated.file_name, attachment.file_name);
        assert_eq!(attachment_updated.description, None);
    }
}
