use apx_core::{
    crypto_eddsa::generate_weak_ed25519_key,
    crypto_rsa::generate_weak_rsa_key,
};

use crate::{
    database::DatabaseClient,
    profiles::test_utils::create_test_remote_profile,
};

use super::{
    queries::{create_invite_code, create_portable_user, create_user},
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
    hostname: &str,
    actor_id: &str,
) -> PortableUser {
    let profile = create_test_remote_profile(
        db_client,
        username,
        hostname,
        actor_id,
    ).await;
    let invite_code = create_invite_code(db_client, None).await.unwrap();
    let user_data = PortableUserData {
        profile_id: profile.id,
        rsa_secret_key: generate_weak_rsa_key().unwrap(),
        ed25519_secret_key: generate_weak_ed25519_key(),
        invite_code: invite_code.to_string(),
    };
    create_portable_user(db_client, user_data).await.unwrap()
}
