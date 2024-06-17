use tokio_postgres::Client;

use mitra_utils::urls::get_hostname;

use super::{
    queries::create_profile,
    types::{
        get_profile_acct,
        DbActor,
        DbActorKey,
        DbActorProfile,
        ProfileCreateData,
    },
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

impl DbActorProfile {
    pub fn local_for_test(username: &str) -> Self {
        let profile = Self {
            username: username.to_string(),
            acct: Some(username.to_string()),
            ..Default::default()
        };
        profile.check_consistency().unwrap();
        profile
    }

    pub fn remote_for_test_with_data(
        username: &str,
        actor_data: DbActor,
    ) -> Self {
        let hostname = get_hostname(&actor_data.id).unwrap();
        let acct = get_profile_acct(username, Some(&hostname));
        let actor_id = actor_data.id.clone();
        let profile = Self {
            username: username.to_string(),
            hostname: Some(hostname.to_string()),
            acct: Some(acct),
            actor_json: Some(actor_data),
            actor_id: Some(actor_id),
            ..Default::default()
        };
        profile.check_consistency().unwrap();
        profile
    }

    pub fn remote_for_test(
        username: &str,
        actor_id: &str,
    ) -> Self {
        let actor_data = DbActor {
            id: actor_id.to_string(),
            ..Default::default()
        };
        Self::remote_for_test_with_data(username, actor_data)
    }
}
