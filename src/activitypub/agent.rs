use mitra_config::Instance;
use mitra_models::{
    profiles::types::PublicKeyType,
    users::types::User,
};
use mitra_utils::crypto_rsa::RsaPrivateKey;

use crate::activitypub::{
    identifiers::{
        local_actor_id,
        local_actor_key_id,
        local_instance_actor_id,
    },
};

pub struct FederationAgent {
    pub user_agent: String,
    // Private instance won't send signed HTTP requests
    pub is_instance_private: bool,

    pub fetcher_timeout: u64,
    pub deliverer_timeout: u64,

    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    pub onion_proxy_url: Option<String>,
    pub i2p_proxy_url: Option<String>,

    pub signer_key: RsaPrivateKey,
    pub signer_key_id: String,
}

impl FederationAgent {
    pub fn new(instance: &Instance) -> Self {
        let instance_actor_id = local_instance_actor_id(&instance.url());
        let instance_actor_key_id = local_actor_key_id(
            &instance_actor_id,
            PublicKeyType::RsaPkcs1,
        );
        Self {
            user_agent: instance.agent(),
            is_instance_private: instance.is_private,
            fetcher_timeout: instance.fetcher_timeout,
            deliverer_timeout: instance.deliverer_timeout,
            proxy_url: instance.proxy_url.clone(),
            onion_proxy_url: instance.onion_proxy_url.clone(),
            i2p_proxy_url: instance.i2p_proxy_url.clone(),
            signer_key: instance.actor_key.clone(),
            signer_key_id: instance_actor_key_id,
        }
    }

    pub fn as_user(instance: &Instance, user: &User) -> Self {
        let mut agent = Self::new(instance);
        agent.signer_key = user.rsa_private_key.clone();
        let actor_id = local_actor_id(&instance.url(), &user.profile.username);
        let actor_key_id = local_actor_key_id(&actor_id, PublicKeyType::RsaPkcs1);
        agent.signer_key_id = actor_key_id;
        agent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_agent_new() {
        let instance_url = "https://social.example";
        let instance = Instance::for_test(instance_url);
        let agent = FederationAgent::new(&instance);
        assert_eq!(agent.user_agent.ends_with(instance_url), true);
        assert_eq!(agent.is_instance_private, true);
        assert_eq!(agent.signer_key, instance.actor_key);
        assert_eq!(agent.signer_key_id, "https://social.example/actor#main-key");
    }
}
