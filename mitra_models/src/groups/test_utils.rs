use apx_core::crypto::{
    eddsa::generate_weak_ed25519_key,
    rsa::generate_weak_rsa_key,
};

use crate::{
    database::DatabaseClient,
    profiles::{
        queries::create_profile,
        types::{
            ActorType,
            DbActorProfile,
            ProfileCreateData,
        },
    },
};

use super::types::GroupCreateData;

impl GroupCreateData {
    pub fn for_test(username: &str) -> Self {
        Self {
            username: username.to_owned(),
            bio: None,
            bio_source: None,
            emojis: vec![],
            rsa_secret_key: generate_weak_rsa_key().unwrap(),
            ed25519_secret_key: generate_weak_ed25519_key(),
        }
    }
}

pub async fn create_test_remote_group(
    db_client: &mut impl DatabaseClient,
    username: &str,
    hostname: &str,
    actor_id: &str,
    is_private: bool,
) -> DbActorProfile {
    let mut group_data = ProfileCreateData::remote_for_test(
        username,
        hostname,
        actor_id,
    );
    group_data.actor_type = ActorType::Group;
    group_data.manually_approves_followers = is_private;
    let group = create_profile(db_client, group_data).await.unwrap();
    group.check_consistency().unwrap();
    group
}
