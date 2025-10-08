use apx_core::url::canonical::CanonicalUrl;
use chrono::{DateTime, Utc};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_utils::files::FileInfo;

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
        DatabaseTypeError,
    },
    media::types::MediaInfo,
};

pub async fn save_activity(
    db_client: &impl DatabaseClient,
    activity_id: &CanonicalUrl,
    activity: &JsonValue,
) -> Result<bool, DatabaseError> {
    // Never overwrite existing object
    // (some servers produce activities and objects with same ID)
    let inserted_count = db_client.execute(
        "
        INSERT INTO activitypub_object (
            object_id,
            object_data
        )
        VALUES ($1, $2)
        ON CONFLICT (object_id)
        DO NOTHING
        ",
        &[&activity_id.to_string(), &activity],
    ).await?;
    let is_new = inserted_count > 0;
    Ok(is_new)
}

pub async fn save_actor(
    db_client: &impl DatabaseClient,
    actor_id: &str,
    actor_json: &JsonValue,
    profile_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO activitypub_object (
            object_id,
            object_data,
            profile_id
        )
        VALUES ($1, $2, $3)
        ON CONFLICT (object_id)
        DO UPDATE SET
            object_data = $2,
            profile_id = $3
        ",
        &[&actor_id, &actor_json, &profile_id],
    ).await?;
    Ok(())
}

pub async fn save_attributed_object(
    db_client: &impl DatabaseClient,
    object_id: &str,
    object_json: &JsonValue,
    post_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO activitypub_object (
            object_id,
            object_data,
            post_id
        )
        VALUES ($1, $2, $3)
        ON CONFLICT (object_id)
        DO UPDATE SET
            object_data = $2,
            post_id = $3
        ",
        &[&object_id, &object_json, &post_id],
    ).await?;
    Ok(())
}

pub async fn get_object_as_target(
    db_client: &impl DatabaseClient,
    object_id: &str,
    target: &str,
) -> Result<JsonValue, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT object_data
        FROM activitypub_object
        WHERE object_id = $1
        AND object_data -> 'to' @> to_jsonb($2::text)
        ",
        &[&object_id, &target],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("activitypub object"))?;
    let object_data = row.try_get("object_data")?;
    Ok(object_data)
}

pub async fn get_actor(
    db_client: &impl DatabaseClient,
    actor_id: &str,
) -> Result<JsonValue, DatabaseError> {
    // Actors can not be private
    let maybe_row = db_client.query_opt(
        "
        SELECT object_data
        FROM activitypub_object
        WHERE object_id = $1 AND profile_id IS NOT NULL
        ",
        &[&actor_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("activitypub object"))?;
    let object_data = row.try_get("object_data")?;
    Ok(object_data)
}

pub async fn delete_activitypub_objects(
    db_client: &impl DatabaseClient,
    created_before: DateTime<Utc>,
) -> Result<u64, DatabaseError> {
    // Don't delete actors, posts and activities in collections
    let deleted_count = db_client.execute(
        "
        DELETE FROM activitypub_object
        WHERE created_at < $1
            AND profile_id IS NULL
            AND post_id IS NULL
            AND NOT EXISTS (
                SELECT 1 FROM activitypub_collection_item
                WHERE object_id = activitypub_object.object_id
            )
        ",
        &[&created_before],
    ).await?;
    Ok(deleted_count)
}

pub async fn add_object_to_collection(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    collection_id: &str,
    object_id: &str,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO activitypub_collection_item (
            owner_id,
            collection_id,
            object_id
        )
        VALUES ($1, $2, $3)
        ON CONFLICT (collection_id, object_id)
        DO NOTHING
        ",
        &[&owner_id, &collection_id, &object_id],
    ).await?;
    Ok(())
}

pub async fn remove_object_from_collection(
    db_client: &impl DatabaseClient,
    collection_id: &str,
    object_id: &str,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        DELETE FROM activitypub_collection_item
        WHERE collection_id = $1 AND object_id = $2
        ",
        &[&collection_id, &object_id],
    ).await?;
    Ok(())
}

pub async fn get_collection_items(
    db_client: &impl DatabaseClient,
    collection_id: &str,
    limit: u32,
) -> Result<Vec<JsonValue>, DatabaseError> {
    // Reverse chronological order
    let rows = db_client.query(
        "
        SELECT activitypub_object.object_data
        FROM activitypub_object
        JOIN activitypub_collection_item USING (object_id)
        WHERE collection_id = $1
        ORDER BY activitypub_object.created_at DESC
        LIMIT $2
        ",
        &[&collection_id, &i64::from(limit)],
    ).await?;
    let items = rows.iter()
        .map(|row| row.try_get("object_data"))
        .collect::<Result<_, _>>()?;
    Ok(items)
}

pub async fn delete_collection_items(
    db_client: &impl DatabaseClient,
    created_before: DateTime<Utc>,
) -> Result<u64, DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM activitypub_object
        WHERE
            created_at < $1
            AND profile_id IS NULL
            AND post_id IS NULL
            AND EXISTS (
                SELECT 1 FROM activitypub_collection_item
                WHERE object_id = activitypub_object.object_id
            )
        ",
        &[&created_before],
    ).await?;
    Ok(deleted_count)
}

pub async fn expand_collections(
    db_client: &impl DatabaseClient,
    audience: &[CanonicalUrl],
) -> Result<Vec<String>, DatabaseError> {
    let audience: Vec<_> = audience.iter()
        .map(|target_id| target_id.to_string())
        .collect();
    let items_rows = db_client.query(
        "
        SELECT collection_id, object_id
        FROM activitypub_collection_item
        WHERE collection_id = ANY($1)
        ",
        &[&audience],
    ).await?;
    let items = items_rows.into_iter()
        .map(|row| {
            let collection_id: String = row.try_get("collection_id")?;
            let object_id: String = row.try_get("object_id")?;
            Ok((collection_id, object_id))
        })
        .collect::<Result<Vec<_>, DatabaseError>>()?;
    let mut expanded_audience = vec![];
    for target_id in audience {
        let collection_items: Vec<_> = items.iter()
            .filter(|(collection_id, _)| *collection_id == target_id)
            .map(|(_, object_id)| object_id.clone())
            .collect();
        if collection_items.len() > 0 {
            // Collection
            expanded_audience.extend(collection_items);
        } else {
            // Object or empty collection
            expanded_audience.push(target_id);
        };
    };
    Ok(expanded_audience)
}

pub async fn create_activitypub_media(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    file_info: FileInfo,
) -> Result<(), DatabaseError> {
    let media_info = MediaInfo::local(file_info);
    db_client.execute(
        "
        INSERT INTO activitypub_media (
            owner_id,
            media
        )
        VALUES ($1, $2)
        ON CONFLICT (owner_id, digest) DO NOTHING
        ",
        &[&owner_id, &media_info],
    ).await?;
    Ok(())
}

pub async fn delete_activitypub_media(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    digest: [u8; 32],
) -> Result<(), DatabaseError> {
    let digest_array_string = format!("{digest:?}");
    let deleted_count = db_client.execute(
        "
        DELETE FROM activitypub_media
        WHERE owner_id = $1 AND digest = $2
        ",
        &[&owner_id, &digest_array_string],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("media"));
    };
    Ok(())
}

pub async fn get_activitypub_media_by_digest(
    db_client: &impl DatabaseClient,
    digest: [u8; 32],
) -> Result<FileInfo, DatabaseError> {
    // Not checking owner
    let digest_array_string = format!("{digest:?}");
    let maybe_row = db_client.query_opt(
        "
        SELECT media
        FROM activitypub_media
        WHERE digest = $1
        LIMIT 1
        ",
        &[&digest_array_string],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("media"))?;
    let media_info = row.try_get("media")?;
    let file_info = match media_info {
        MediaInfo::File { file_info, .. } => file_info,
        _ => return Err(DatabaseTypeError.into()),
    };
    Ok(file_info)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        posts::test_utils::create_test_remote_post,
        profiles::test_utils::create_test_remote_profile,
        users::test_utils::create_test_portable_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_save_activity() {
        let db_client = &create_test_database().await;
        let activity_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/activities/1";
        let canonical_activity_id = CanonicalUrl::parse_canonical(activity_id).unwrap();
        let activity = json!({
            "id": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/activities/1",
            "actor": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "type": "Create",
            "object": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1",
        });
        // Create
        let is_new = save_activity(
            db_client,
            &canonical_activity_id,
            &activity,
        ).await.unwrap();
        assert!(is_new);
        // Update
        let is_new = save_activity(
            db_client,
            &canonical_activity_id,
            &activity,
        ).await.unwrap();
        assert!(!is_new);
    }

    #[tokio::test]
    #[serial]
    async fn test_save_actor() {
        let db_client = &mut create_test_database().await;
        let canonical_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let profile = create_test_remote_profile(
            db_client,
            "test",
            "social.example",
            canonical_id,
        ).await;

        // Create
        let actor_json = json!({
            "type": "Person",
            "id": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "inbox": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/inbox",
            "outbox": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/outbox",
            "name": "test-1",
        });
        save_actor(db_client, canonical_id, &actor_json, profile.id).await.unwrap();

        // Update
        let actor_json = json!({
            "type": "Person",
            "id": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "inbox": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/inbox",
            "outbox": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/outbox",
            "name": "test-2",
        });
        save_actor(db_client, canonical_id, &actor_json, profile.id).await.unwrap();

        // Get
        let actor_json_stored = get_actor(db_client, canonical_id).await.unwrap();
        assert_eq!(actor_json_stored, actor_json);
    }

    #[tokio::test]
    #[serial]
    async fn test_save_attributed_object() {
        let db_client = &mut create_test_database().await;
        let canonical_actor_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let profile = create_test_remote_profile(
            db_client,
            "test",
            "social.example",
            canonical_actor_id,
        ).await;
        let canonical_object_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1";
        let post = create_test_remote_post(
            db_client,
            profile.id,
            "test",
            canonical_object_id,
        ).await;

        // Create
        let object_json = json!({
            "type": "Note",
            "id": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1",
            "attributedTo": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "content": "test",
            "to": ["https://www.w3.org/ns/activitystreams#Public"],
        });
        save_attributed_object(
            db_client,
            canonical_object_id,
            &object_json,
            post.id,
        ).await.unwrap();

        // Get
        let object_json_stored = get_object_as_target(
            db_client,
            canonical_object_id,
            "https://www.w3.org/ns/activitystreams#Public",
        ).await.unwrap();
        assert_eq!(object_json_stored, object_json);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_object_as_target() {
        let db_client = &create_test_database().await;
        let activity_id = "https://social.example/activities/123";
        let canonical_activity_id = CanonicalUrl::parse_canonical(activity_id).unwrap();
        let target_id = "https://social.example/users/2";
        let activity = json!({
            "id": activity_id,
            "type": "Like",
            "actor": "https://social.example/users/1",
            "object": "https://social.example/objects/321",
            "to": [target_id],
        });
        save_activity(
            db_client,
            &canonical_activity_id,
            &activity,
        ).await.unwrap();
        let activity_found = get_object_as_target(
            db_client,
            activity_id,
            target_id,
        ).await.unwrap();
        assert_eq!(activity_found, activity);
        let error = get_object_as_target(
            db_client,
            activity_id,
            "https://www.w3.org/ns/activitystreams#Public",
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "activitypub object not found");
    }

    #[tokio::test]
    #[serial]
    async fn test_add_object_to_collection() {
        let db_client = &mut create_test_database().await;
        let canonical_actor_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let user = create_test_portable_user(
            db_client,
            "test",
            canonical_actor_id,
        ).await;
        // Create activity
        let activity_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/activities/321";
        let canonical_activity_id = CanonicalUrl::parse_canonical(activity_id).unwrap();
        let activity = json!({
            "id": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/activities/321",
            "type": "Create",
            "actor": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "object": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/321",
        });
        save_activity(db_client, &canonical_activity_id, &activity).await.unwrap();

        // Add to collection
        let canonical_collection_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor/outbox";
        add_object_to_collection(
            db_client,
            user.id,
            canonical_collection_id,
            &canonical_activity_id.to_string(),
        ).await.unwrap();
        // Re-add
        add_object_to_collection(
            db_client,
            user.id,
            canonical_collection_id,
            &canonical_activity_id.to_string(),
        ).await.unwrap();
        // Read collection
        let items = get_collection_items(
            db_client,
            canonical_collection_id,
            10,
        ).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], activity);

        // Remove from collection
        remove_object_from_collection(
            db_client,
            canonical_collection_id,
            &canonical_activity_id.to_string(),
        ).await.unwrap();
        let items = get_collection_items(
            db_client,
            canonical_collection_id,
            10,
        ).await.unwrap();
        assert_eq!(items.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_expand_collections() {
        let db_client = &create_test_database().await;
        let actor_id = "https://social.example/actor";
        let audience = vec![
            CanonicalUrl::parse_canonical(actor_id).unwrap(),
        ];
        let expanded_audience = expand_collections(
            db_client,
            &audience,
        ).await.unwrap();
        assert_eq!(expanded_audience.len(), 1);
        assert_eq!(expanded_audience[0], actor_id);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_activitypub_media() {
        let db_client = &mut create_test_database().await;
        let canonical_actor_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor";
        let user = create_test_portable_user(
            db_client,
            "test",
            canonical_actor_id,
        ).await;
        let media_info = MediaInfo::png_for_test();
        let MediaInfo::File { file_info, .. } = &media_info else {
            unreachable!();
        };
        create_activitypub_media(
            db_client,
            user.id,
            file_info.clone(),
        ).await.unwrap();

        let db_file_info = get_activitypub_media_by_digest(
            db_client,
            file_info.digest,
        ).await.unwrap();
        assert_eq!(db_file_info.file_name, file_info.file_name);
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_activitypub_media() {
        let db_client = &mut create_test_database().await;
        let user = create_test_portable_user(
            db_client,
            "test",
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
        ).await;
        let MediaInfo::File { file_info, .. } = MediaInfo::png_for_test() else {
            unreachable!();
        };
        create_activitypub_media(
            db_client,
            user.id,
            file_info.clone(),
        ).await.unwrap();

        delete_activitypub_media(
            db_client,
            user.id,
            file_info.digest,
        ).await.unwrap();
    }
}
