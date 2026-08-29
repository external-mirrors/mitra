use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
    relationships::{
        queries::has_relationship,
        types::RelationshipType,
    },
};

pub async fn can_delete_group_post(
    db_client: &impl DatabaseClient,
    actor: &DbActorProfile,
    group: &DbActorProfile,
) -> Result<bool, DatabaseError> {
    let can_delete = {
        actor.id == group.id || has_relationship(
            db_client,
            actor.id,
            group.id,
            RelationshipType::GroupAdmin,
        ).await?
    };
    Ok(can_delete)
}
