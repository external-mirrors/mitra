use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
};

pub async fn get_object_ids(
    db_client: &impl DatabaseClient,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile.actor_id AS object_id
        FROM actor_profile
        WHERE actor_profile.actor_json IS NOT NULL
        UNION ALL
        SELECT post.object_id
        FROM post
        WHERE post.object_id IS NOT NULL
        ",
        &[],
    ).await?;
    let object_ids = rows.iter()
        .map(|row| row.try_get("object_id"))
        .collect::<Result<_, _>>()?;
    Ok(object_ids)
}
