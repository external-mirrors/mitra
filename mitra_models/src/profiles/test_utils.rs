use apx_core::{
    ap_url::is_ap_url,
    http_url_whatwg::get_hostname,
};
use uuid::Uuid;

use crate::{
    database::DatabaseClient,
    users::test_utils::create_test_user,
};

use super::{
    queries::create_profile,
    types::{
        DbActor,
        DbActorKey,
        DbActorProfile,
        ProfileCreateData,
        WebfingerHostname,
    },
};

impl ProfileCreateData {
    pub fn remote_for_test(
        username: &str,
        hostname: &str,
        actor_id: &str,
    ) -> Self {
        let mut db_actor = DbActor {
            id: actor_id.to_string(),
            ..Default::default()
        };
        if is_ap_url(&db_actor.id) {
            db_actor.gateways.push(format!("https://{hostname}"));
        };
        let hostname = if hostname.ends_with(".local") {
            // Special case: creating unmanaged account
            WebfingerHostname::Unknown
        } else {
            WebfingerHostname::Remote(hostname.to_string())
        };
        ProfileCreateData {
            username: username.to_string(),
            hostname: hostname,
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(db_actor),
            ..Default::default()
        }
    }
}

pub async fn create_test_local_profile(
    db_client: &mut impl DatabaseClient,
    username: &str,
) -> DbActorProfile {
    let user = create_test_user(db_client, username).await;
    user.profile
}

pub async fn create_test_remote_profile(
    db_client: &mut impl DatabaseClient,
    username: &str,
    hostname: &str,
    actor_id: &str,
) -> DbActorProfile {
    let profile_data = ProfileCreateData::remote_for_test(
        username,
        hostname,
        actor_id,
    );
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
        let acct = format!("{}@{}", username, hostname);
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
