use crate::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
};

use super::{
    queries::get_user_by_name,
    types::User,
};

pub async fn get_user_by_name_with_pool(
    db_pool: &DatabaseConnectionPool,
    username: &str,
) -> Result<User, DatabaseError> {
    let db_client = &**get_database_client(db_pool).await?;
    get_user_by_name(db_client, username).await
}
