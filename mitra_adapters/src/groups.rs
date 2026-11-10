use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
    groups::helpers::can_manage_group,
};

pub async fn can_delete_group_post(
    db_client: &impl DatabaseClient,
    actor: &DbActorProfile,
    group: &DbActorProfile,
) -> Result<bool, DatabaseError> {
    let can_delete = {
        actor.id == group.id || can_manage_group(
            db_client,
            actor.id,
            group.id,
        ).await?
    };
    Ok(can_delete)
}
