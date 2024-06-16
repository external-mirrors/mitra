use tokio_postgres::Client;

use super::{
    queries::create_profile,
    types::{DbActor, DbActorKey, DbActorProfile, ProfileCreateData},
};

pub async fn create_test_remote_profile(
    db_client: &mut Client,
    username: &str,
    hostname: &str,
    actor_id: &str,
) -> DbActorProfile {
    let profile_data = ProfileCreateData {
        username: username.to_string(),
        hostname: Some(hostname.to_string()),
        public_keys: vec![DbActorKey::default()],
        actor_json: Some(DbActor {
            id: actor_id.to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let profile = create_profile(db_client, profile_data).await.unwrap();
    profile.check_consistency().unwrap();
    profile
}
