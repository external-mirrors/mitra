use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};

use super::{
    queries::{get_user_by_id, get_user_by_name},
    types::User,
};

pub async fn get_user_by_id_or_name(
    db_client: &impl DatabaseClient,
    user_id_or_name: &str,
) -> Result<User, DatabaseError> {
    if let Ok(user_id) = Uuid::parse_str(user_id_or_name) {
        get_user_by_id(db_client, user_id).await
    } else {
        get_user_by_name(db_client, user_id_or_name).await
    }
}
