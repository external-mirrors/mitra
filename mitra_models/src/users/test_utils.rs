use apx_core::{
    crypto::{
        eddsa::generate_weak_ed25519_key,
        rsa::generate_weak_rsa_key,
    },
};

use crate::{
    database::DatabaseClient,
    profiles::test_utils::create_test_remote_profile,
};

use super::{
    queries::{create_portable_user, create_user},
    types::{PortableUser, PortableUserData, User, UserCreateData},
};

pub async fn create_test_user(
    db_client: &mut impl DatabaseClient,
    username: &str,
) -> User {
    let user_data = UserCreateData {
        username: username.to_string(),
        password_digest: Some("test".to_string()),
        ..Default::default()
    };
    create_user(db_client, user_data).await.unwrap()
}

pub async fn create_test_portable_user(
    db_client: &mut impl DatabaseClient,
    username: &str,
    actor_id: &str,
) -> PortableUser {
    let profile = create_test_remote_profile(
        db_client,
        username,
        "server.local", // local webfinger
        actor_id,
    ).await;
    let user_data = PortableUserData {
        profile_id: profile.id,
        rsa_secret_key: generate_weak_rsa_key().unwrap(),
        ed25519_secret_key: generate_weak_ed25519_key(),
        invite_code: None,
    };
    create_portable_user(db_client, user_data).await.unwrap()
}
