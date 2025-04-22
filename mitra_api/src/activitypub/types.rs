use serde::Serialize;

use mitra_activitypub::actors::keys::Multikey;
use mitra_models::users::types::PortableUser;

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct PortableActorKeys {
    assertion_method: Vec<Multikey>,
}

impl PortableActorKeys {
    pub fn new(user: PortableUser) -> Self {
        let actor_id = user.profile.expect_remote_actor_id();
        let assertion_method = vec![
            Multikey::build_rsa(actor_id, &user.rsa_secret_key)
                .expect("RSA key should be serializable"),
            Multikey::build_ed25519(actor_id, &user.ed25519_secret_key),
        ];
        Self { assertion_method }
    }
}
