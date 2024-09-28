use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::{
    database::{
        catch_unique_violation,
        DatabaseClient,
        DatabaseError,
    },
    posts::types::Visibility,
    relationships::types::RelationshipType,
};

use super::types::Conversation;

pub async fn create_conversation(
    db_client: &impl DatabaseClient,
    root_id: Uuid,
    audience: Option<&str>,
) -> Result<Conversation, DatabaseError> {
    let conversation_id = generate_ulid();
    let row = db_client.query_one(
        "
        INSERT INTO conversation (
            id,
            root_id,
            audience
        )
        VALUES ($1, $2, $3)
        RETURNING conversation
        ",
        &[
            &conversation_id,
            &root_id,
            &audience,
        ],
    ).await.map_err(catch_unique_violation("conversation"))?;
    let conversation = row.try_get("conversation")?;
    Ok(conversation)
}

pub async fn get_conversation(
    db_client: &impl DatabaseClient,
    conversation_id: Uuid,
) -> Result<Conversation, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT conversation FROM conversation
        WHERE conversation.id = $1
        ",
        &[&conversation_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("conversation"))?;
    let conversation = row.try_get("conversation")?;
    Ok(conversation)
}

pub async fn is_conversation_participant(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<bool, DatabaseError> {
    let statement = format!(
        "
        SELECT 1
        FROM conversation
        JOIN post AS root ON conversation.root_id = root.id
        WHERE
            conversation.id = $2
            AND (
                root.author_id = $1
                OR EXISTS (
                    SELECT 1 FROM relationship
                    WHERE
                        relationship.source_id = $1
                        AND relationship.target_id = root.author_id
                        AND (
                            root.visibility = {visibility_followers}
                            AND relationship_type = {relationship_follow}
                        )
                )
            )
        ",
        visibility_followers=i16::from(Visibility::Followers),
        relationship_follow=i16::from(RelationshipType::Follow),
    );
    let maybe_row = db_client.query_opt(
        &statement,
        &[&user_id, &conversation_id],
    ).await?;
    Ok(maybe_row.is_some())
}
