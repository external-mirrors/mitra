use tokio_postgres::Client;
use uuid::Uuid;

use mitra_utils::{
    ap_url::is_ap_url,
    urls::get_hostname,
};

use crate::users::test_utils::create_test_user;

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

pub async fn create_test_local_profile(
    db_client: &mut Client,
    username: &str,
) -> DbActorProfile {
    let user = create_test_user(db_client, username).await;
    user.profile
}

pub async fn create_test_remote_profile(
    db_client: &mut Client,
    username: &str,
    hostname: &str,
    actor_id: &str,
) -> DbActorProfile {
    let mut db_actor = DbActor {
        id: actor_id.to_string(),
        ..Default::default()
    };
    if is_ap_url(&db_actor.id) {
        db_actor.gateways.push(format!("https://{hostname}"));
    };
    let profile_data = ProfileCreateData {
        username: username.to_string(),
        hostname: Some(hostname.to_string()),
        public_keys: vec![DbActorKey::default()],
        actor_json: Some(db_actor),
        ..Default::default()
    };
    let profile = create_profile(db_client, profile_data).await.unwrap();
    profile.check_consistency().unwrap();
    profile
}

impl DbActorProfile {
    pub fn local_for_test(username: &str) -> Self {
        let id = Uuid::new_v4();
        let profile = Self {
            id: id,
            user_id: Some(id),
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
        let hostname = if actor_data.is_portable() {
            get_hostname(&actor_data.gateways.first().unwrap()).unwrap()
        } else {
            get_hostname(&actor_data.id).unwrap()
        };
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
