use mitra_models::{
    activitypub::queries::{
        add_object_to_collection,
        remove_object_from_collection,
    },
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
};

pub async fn add_follower(
    db_client: &impl DatabaseClient,
    source: &DbActorProfile,
    target: &DbActorProfile,
) -> Result<(), DatabaseError> {
    // Local actors are not tracked (deliveries happen automatically)
    let Some(ref source_data) = source.actor_json else {
        return Ok(());
    };
    let Some(ref target_data) = target.actor_json else {
        return Ok(());
    };
    let Some(ref collection_id) = target_data.followers else {
        return Ok(());
    };
    add_object_to_collection(
        db_client,
        target.id,
        collection_id,
        &source_data.id,
    ).await?;
    log::info!("added actor to followers collection");
    Ok(())
}

pub async fn remove_follower(
    db_client: &impl DatabaseClient,
    source: &DbActorProfile,
    target: &DbActorProfile,
) -> Result<(), DatabaseError> {
    let Some(ref source_data) = source.actor_json else {
        return Ok(());
    };
    let Some(ref target_data) = target.actor_json else {
        return Ok(());
    };
    let Some(ref collection_id) = target_data.followers else {
        return Ok(());
    };
    remove_object_from_collection(
        db_client,
        collection_id,
        &source_data.id,
    ).await?;
    log::info!("removed actor from followers collection");
    Ok(())
}
