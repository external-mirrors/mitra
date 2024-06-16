use tokio_postgres::Client;

use super::{
    queries::create_user,
    types::{User, UserCreateData},
};

pub async fn create_test_user(db_client: &mut Client, username: &str) -> User {
    let user_data = UserCreateData {
        username: username.to_string(),
        password_hash: Some("test".to_string()),
        ..Default::default()
    };
    create_user(db_client, user_data).await.unwrap()
}
