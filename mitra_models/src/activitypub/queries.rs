use chrono::{DateTime, Utc};
use serde_json::{Value as JsonValue};

use crate::database::{
    DatabaseClient,
    DatabaseError,
};

pub async fn save_activity(
    db_client: &impl DatabaseClient,
    activity_id: &str,
    activity: &JsonValue,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO activitypub_object (
            object_id,
            object_data
        )
        VALUES ($1, $2)
        ON CONFLICT (object_id)
        DO UPDATE SET object_data = $2
        ",
        &[&activity_id, &activity],
    ).await?;
    Ok(())
}

pub async fn delete_activitypub_objects(
    db_client: &impl DatabaseClient,
    created_before: DateTime<Utc>,
) -> Result<u64, DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM activitypub_object
        WHERE created_at < $1
        ",
        &[&created_before],
    ).await?;
    Ok(deleted_count)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_save_activity() {
        let db_client = &mut create_test_database().await;
        let canonical_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/activities/1";
        let activity = json!({
            "id": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/activities/1",
            "actor": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
            "type": "Create",
            "object": "https://social.example/.well-known/apgateway/did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/objects/1",
        });
        // Create
        save_activity(db_client, canonical_id, &activity).await.unwrap();
        // Update
        save_activity(db_client, canonical_id, &activity).await.unwrap();
    }
}
