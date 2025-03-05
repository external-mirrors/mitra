use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::database::{
    DatabaseClient,
    DatabaseError,
};
use crate::instances::queries::create_instance;
use crate::media::types::DeletionQueue;
use crate::profiles::queries::update_emoji_caches;

use super::types::{DbEmoji, EmojiImage};

/// Creates emoji or updates emoji with matching `emoji_name` and `hostname`.
/// `object_id` is replaced on update.
pub async fn create_or_update_remote_emoji(
    db_client: &mut impl DatabaseClient,
    emoji_name: &str,
    hostname: &str,
    image: EmojiImage,
    object_id: &str,
    updated_at: DateTime<Utc>,
) -> Result<(DbEmoji, DeletionQueue), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_prev_image_row = transaction.query_opt(
        "
        SELECT image ->> 'file_name' AS file_name
        FROM emoji WHERE emoji_name = $1 AND hostname = $2
        FOR UPDATE
        ",
        &[&emoji_name, &hostname],
    ).await?;
    let maybe_prev_image = match maybe_prev_image_row {
        Some(prev_image_row) => prev_image_row.try_get("file_name")?,
        None => None,
    };
    create_instance(&transaction, hostname).await?;
    let emoji_id = generate_ulid();
    // Not expecting conflict on object_id
    let row = transaction.query_one(
        "
        INSERT INTO emoji (
            id,
            emoji_name,
            hostname,
            image,
            object_id,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (emoji_name, hostname)
        DO UPDATE SET
            image = $4,
            object_id = $5,
            updated_at = $6
        RETURNING emoji
        ",
        &[
            &emoji_id,
            &emoji_name,
            &hostname,
            &image,
            &object_id,
            &updated_at,
        ],
    ).await?;
    let emoji: DbEmoji = row.try_get("emoji")?;
    update_emoji_caches(&transaction, emoji.id).await?;
    transaction.commit().await?;
    let deletion_queue = DeletionQueue {
        files: maybe_prev_image.map(|name| vec![name]).unwrap_or_default(),
        ipfs_objects: vec![],
    };
    Ok((emoji, deletion_queue))
}

pub async fn update_emoji(
    db_client: &mut impl DatabaseClient,
    emoji_id: Uuid,
    image: EmojiImage,
    updated_at: DateTime<Utc>,
) -> Result<(DbEmoji, DeletionQueue), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let prev_image_row = transaction.query_one(
        "
        SELECT image ->> 'file_name' AS file_name
        FROM emoji WHERE id = $1
        FOR UPDATE
        ",
        &[&emoji_id],
    ).await?;
    let prev_image = prev_image_row.try_get("file_name")?;
    let row = transaction.query_one(
        "
        UPDATE emoji
        SET
            image = $1,
            updated_at = $2
        WHERE id = $3
        RETURNING emoji
        ",
        &[
            &image,
            &updated_at,
            &emoji_id,
        ],
    ).await?;
    let emoji: DbEmoji = row.try_get("emoji")?;
    update_emoji_caches(&transaction, emoji.id).await?;
    transaction.commit().await?;
    let deletion_queue = DeletionQueue {
        files: vec![prev_image],
        ipfs_objects: vec![],
    };
    Ok((emoji, deletion_queue))
}

pub async fn create_or_update_local_emoji(
    db_client: &mut impl DatabaseClient,
    emoji_name: &str,
    image: EmojiImage,
) -> Result<(DbEmoji, DeletionQueue), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_prev_image_row = transaction.query_opt(
        "
        SELECT image ->> 'file_name' AS file_name
        FROM emoji WHERE emoji_name = $1 AND hostname IS NULL
        FOR UPDATE
        ",
        &[&emoji_name],
    ).await?;
    let maybe_prev_image = match maybe_prev_image_row {
        Some(prev_image_row) => prev_image_row.try_get("file_name")?,
        None => None,
    };
    let emoji_id = generate_ulid();
    // Partial index on emoji_name is used
    // UNIQUE NULLS NOT DISTINCT requires Postgresql 15+
    let row = transaction.query_one(
        "
        INSERT INTO emoji (
            id,
            emoji_name,
            image,
            updated_at
        )
        VALUES ($1, $2, $3, CURRENT_TIMESTAMP)
        ON CONFLICT (emoji_name) WHERE hostname IS NULL
        DO UPDATE SET image = $3, updated_at = CURRENT_TIMESTAMP
        RETURNING emoji
        ",
        &[&emoji_id, &emoji_name, &image],
    ).await?;
    let emoji: DbEmoji = row.try_get("emoji")?;
    update_emoji_caches(&transaction, emoji.id).await?;
    transaction.commit().await?;
    let deletion_queue = DeletionQueue {
        files: maybe_prev_image.map(|name| vec![name]).unwrap_or_default(),
        ipfs_objects: vec![],
    };
    Ok((emoji, deletion_queue))
}

pub async fn get_local_emoji_by_name(
    db_client: &impl DatabaseClient,
    emoji_name: &str,
) -> Result<DbEmoji, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT emoji
        FROM emoji
        WHERE hostname IS NULL AND emoji_name = $1
        ",
        &[&emoji_name],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("emoji"))?;
    let emoji = row.try_get("emoji")?;
    Ok(emoji)
}

pub async fn get_local_emojis_by_names(
    db_client: &impl DatabaseClient,
    names: &[String],
) -> Result<Vec<DbEmoji>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT emoji
        FROM emoji
        WHERE hostname IS NULL AND emoji_name = ANY($1)
        ",
        &[&names],
    ).await?;
    let emojis = rows.iter()
        .map(|row| row.try_get("emoji"))
        .collect::<Result<_, _>>()?;
    Ok(emojis)
}

pub async fn get_local_emojis(
    db_client: &impl DatabaseClient,
) -> Result<Vec<DbEmoji>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT emoji
        FROM emoji
        WHERE hostname IS NULL
        ORDER BY emoji_name
        ",
        &[],
    ).await?;
    let emojis = rows.iter()
        .map(|row| row.try_get("emoji"))
        .collect::<Result<_, _>>()?;
    Ok(emojis)
}

pub async fn get_emoji_by_name_and_hostname(
    db_client: &impl DatabaseClient,
    emoji_name: &str,
    hostname: &str,
) -> Result<DbEmoji, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT emoji
        FROM emoji WHERE emoji_name = $1 AND hostname = $2
        ",
        &[&emoji_name, &hostname],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("emoji"))?;
    let emoji = row.try_get("emoji")?;
    Ok(emoji)
}

pub async fn get_remote_emoji_by_object_id(
    db_client: &impl DatabaseClient,
    object_id: &str,
) -> Result<DbEmoji, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT emoji
        FROM emoji WHERE object_id = $1
        ",
        &[&object_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("emoji"))?;
    let emoji = row.try_get("emoji")?;
    Ok(emoji)
}

pub async fn delete_emoji(
    db_client: &impl DatabaseClient,
    emoji_id: Uuid,
) -> Result<DeletionQueue, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        DELETE FROM emoji WHERE id = $1
        RETURNING emoji
        ",
        &[&emoji_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("emoji"))?;
    let emoji: DbEmoji = row.try_get("emoji")?;
    update_emoji_caches(db_client, emoji.id).await?;
    Ok(DeletionQueue {
        files: vec![emoji.image.file_name],
        ipfs_objects: vec![],
    })
}

pub async fn find_unused_remote_emojis(
    db_client: &impl DatabaseClient,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT emoji.id
        FROM emoji
        WHERE
            emoji.object_id IS NOT NULL
            AND NOT EXISTS (
                SELECT 1
                FROM post_emoji
                WHERE post_emoji.emoji_id = emoji.id
            )
            AND NOT EXISTS (
                SELECT 1
                FROM post_reaction
                WHERE post_reaction.emoji_id = emoji.id
            )
            AND NOT EXISTS (
                SELECT 1
                FROM profile_emoji
                WHERE profile_emoji.emoji_id = emoji.id
            )
        ",
        &[],
    ).await?;
    let ids: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        media::types::MediaInfo,
        profiles::{
            queries::{create_profile, get_profile_by_id},
            types::ProfileCreateData,
        },
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_remote_emoji() {
        let db_client = &mut create_test_database().await;
        let emoji_name = "test";
        let hostname = "example.org";
        let image = EmojiImage::from(MediaInfo::png_for_test());
        let object_id = "https://example.org/emojis/test";
        let updated_at = Utc::now();
        let (emoji, deletion_queue) = create_or_update_remote_emoji(
            db_client,
            emoji_name,
            hostname,
            image.clone(),
            object_id,
            updated_at,
        ).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 0);

        let emoji_id = emoji.id;
        let emoji = get_remote_emoji_by_object_id(
            db_client,
            object_id,
        ).await.unwrap();
        assert_eq!(emoji.id, emoji_id);
        assert_eq!(emoji.emoji_name, emoji_name);
        assert_eq!(emoji.hostname, Some(hostname.to_string()));
        assert_eq!(emoji.image.media_type, "image/png");
        assert_eq!(emoji.object_id.unwrap(), object_id);

        // New ID
        let object_id = "https://example.org/emojis/test?hash=12345";
        let (emoji, deletion_queue) = create_or_update_remote_emoji(
            db_client,
            emoji_name,
            hostname,
            image,
            object_id,
            updated_at,
        ).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 1);
        assert_eq!(emoji.id, emoji_id);
        assert_eq!(emoji.object_id.unwrap(), object_id);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_remote_emoji() {
        let db_client = &mut create_test_database().await;
        let image = EmojiImage::from(MediaInfo::png_for_test());
        let (emoji, _) = create_or_update_remote_emoji(
            db_client,
            "test",
            "example.social",
            image.clone(),
            "https://example.social/emojis/test",
            Utc::now(),
        ).await.unwrap();
        let (updated_emoji, deletion_queue) = update_emoji(
            db_client,
            emoji.id,
            image,
            Utc::now(),
        ).await.unwrap();
        assert_ne!(updated_emoji.updated_at, emoji.updated_at);
        assert_eq!(deletion_queue.files.len(), 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_or_update_local_emoji() {
        let db_client = &mut create_test_database().await;
        let image = EmojiImage::from(MediaInfo::png_for_test());
        let (emoji, deletion_queue) = create_or_update_local_emoji(
            db_client,
            "local",
            image.clone(),
        ).await.unwrap();
        assert_eq!(emoji.hostname.is_none(), true);
        assert_eq!(deletion_queue.files.len(), 0);
        let (updated_emoji, deletion_queue) = create_or_update_local_emoji(
            db_client,
            "local",
            image,
        ).await.unwrap();
        assert_eq!(updated_emoji.id, emoji.id);
        assert_eq!(updated_emoji.hostname.is_none(), true);
        assert_ne!(updated_emoji.updated_at, emoji.updated_at);
        assert_eq!(deletion_queue.files.len(), 1);
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_emoji() {
        let db_client = &mut create_test_database().await;
        let image = EmojiImage::from(MediaInfo::png_for_test());
        let (emoji, _) = create_or_update_local_emoji(
            db_client,
            "test",
            image,
        ).await.unwrap();
        let deletion_queue = delete_emoji(db_client, emoji.id).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 1);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_emoji_and_update_caches() {
        let db_client = &mut create_test_database().await;
        let image = EmojiImage::from(MediaInfo::png_for_test());
        let (emoji, _) = create_or_update_local_emoji(
            db_client,
            "test",
            image,
        ).await.unwrap();
        let profile_data = ProfileCreateData {
            emojis: vec![emoji.id],
            ..ProfileCreateData::remote_for_test(
                "test",
                "social.example",
                "https://social.example/actor",
            )
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        assert_eq!(profile.emojis.into_inner().len(), 1);
        delete_emoji(db_client, emoji.id).await.unwrap();
        let profile = get_profile_by_id(db_client, profile.id).await.unwrap();
        assert_eq!(profile.emojis.into_inner().len(), 0);
    }
}
