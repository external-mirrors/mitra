use apx_core::crypto_rsa::RsaSecretKey;
use apx_sdk::agent::{FederationAgent, HttpSigner};

use mitra_config::Instance;
use mitra_models::{
    profiles::types::PublicKeyType,
    users::types::User,
};

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
    // Public instances should set User-Agent header
    let maybe_user_agent = if instance.federation.enabled {
        Some(instance.agent())
    } else {
        None
    };
    // Public instances should sign requests
    let maybe_signer = if instance.federation.enabled {
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);
        Some(signer)
    } else {
        None
    };
    FederationAgent {
        user_agent: maybe_user_agent,
        ssrf_protection_enabled: instance.federation.ssrf_protection_enabled,
        response_size_limit: RESPONSE_SIZE_LIMIT,
        fetcher_timeout: instance.federation.fetcher_timeout,
        deliverer_timeout: instance.federation.deliverer_timeout,
        proxy_url: instance.federation.proxy_url.clone(),
        onion_proxy_url: instance.federation.onion_proxy_url.clone(),
        i2p_proxy_url: instance.federation.i2p_proxy_url.clone(),
        signer: maybe_signer,
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
        let instance_actor_key = instance.rsa_secret_key.clone();
        (instance_actor_key, instance_actor_key_id)
    };
    build_federation_agent_with_key(instance, signer_key, signer_key_id)
}

#[cfg(test)]
mod tests {
    use apx_core::crypto::common::SecretKey;
    use super::*;

    #[test]
    fn test_build_federation_agent_private() {
        let instance_url = "https://social.example";
        let instance = Instance::for_test(instance_url);
        let agent = build_federation_agent(&instance, None);
        assert_eq!(agent.user_agent.is_none(), true);
        assert_eq!(agent.ssrf_protection_enabled, true);
        assert_eq!(agent.response_size_limit, RESPONSE_SIZE_LIMIT);
        assert_eq!(agent.signer.is_none(), true);
    }

    #[test]
    fn test_build_federation_agent() {
        let instance_url = "https://social.example";
        let mut instance = Instance::for_test(instance_url);
        instance.federation.enabled = true;
        let agent = build_federation_agent(&instance, None);
        assert_eq!(agent.user_agent.unwrap().ends_with(instance_url), true);
        assert_eq!(agent.ssrf_protection_enabled, true);
        assert_eq!(agent.response_size_limit, RESPONSE_SIZE_LIMIT);
        let request_signer = agent.signer.unwrap();
        let SecretKey::Rsa(secret_key) = request_signer.key else {
            panic!("unexpected key type");
        };
        assert_eq!(secret_key, instance.rsa_secret_key);
        assert_eq!(request_signer.key_id, "https://social.example/actor#main-key");
    }
}
