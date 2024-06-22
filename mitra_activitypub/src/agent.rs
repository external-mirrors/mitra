use mitra_config::Instance;
use mitra_federation::agent::FederationAgent;
use mitra_models::{
    profiles::types::PublicKeyType,
    users::types::User,
};
use mitra_utils::crypto_rsa::RsaSecretKey;

use super::{
    identifiers::{
        local_actor_id,
        local_actor_key_id,
        local_instance_actor_id,
    },
};

// Roughly equals to content size limit * collection size limit
// See also: mitra_validators::posts::CONTENT_MAX_SIZE
const RESPONSE_SIZE_LIMIT: usize = 2_000_000;

pub(super) fn build_federation_agent_with_key(
    instance: &Instance,
    signer_key: RsaSecretKey,
    signer_key_id: String,
) -> FederationAgent {
    FederationAgent {
        user_agent: instance.agent(),
        is_instance_private: instance.is_private,
        protect_localhost: true,
        response_size_limit: RESPONSE_SIZE_LIMIT,
        fetcher_timeout: instance.fetcher_timeout,
        deliverer_timeout: instance.deliverer_timeout,
        deliverer_log_response_length: instance.deliverer_log_response_length,
        proxy_url: instance.proxy_url.clone(),
        onion_proxy_url: instance.onion_proxy_url.clone(),
        i2p_proxy_url: instance.i2p_proxy_url.clone(),
        signer_key: signer_key,
        signer_key_id: signer_key_id,
    }
}

pub fn build_federation_agent(
    instance: &Instance,
    maybe_user: Option<&User>,
) -> FederationAgent {
    let (signer_key, signer_key_id) = if let Some(user) = maybe_user {
        let actor_key = user.rsa_secret_key.clone();
        let actor_id = local_actor_id(&instance.url(), &user.profile.username);
        let actor_key_id = local_actor_key_id(&actor_id, PublicKeyType::RsaPkcs1);
        (actor_key, actor_key_id)
    } else {
        let instance_actor_id = local_instance_actor_id(&instance.url());
        let instance_actor_key_id = local_actor_key_id(
            &instance_actor_id,
            PublicKeyType::RsaPkcs1,
        );
        let instance_actor_key = instance.actor_rsa_key.clone();
        (instance_actor_key, instance_actor_key_id)
    };
    build_federation_agent_with_key(instance, signer_key, signer_key_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_federation_agent() {
        let instance_url = "https://social.example";
        let instance = Instance::for_test(instance_url);
        let agent = build_federation_agent(&instance, None);
        assert_eq!(agent.user_agent.ends_with(instance_url), true);
        assert_eq!(agent.is_instance_private, true);
        assert_eq!(agent.protect_localhost, true);
        assert_eq!(agent.response_size_limit, RESPONSE_SIZE_LIMIT);
        assert_eq!(agent.signer_key, instance.actor_rsa_key);
        assert_eq!(agent.signer_key_id, "https://social.example/actor#main-key");
    }
}
