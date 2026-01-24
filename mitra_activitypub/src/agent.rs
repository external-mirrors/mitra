use apx_core::crypto::rsa::RsaSecretKey;
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
        rfc9421_enabled: false,
    }
}

pub fn build_federation_agent(
    instance: &Instance,
    maybe_user: Option<&User>,
) -> FederationAgent {
    let (signer_key, signer_key_id) = if let Some(user) = maybe_user {
        let actor_key = user.rsa_secret_key.clone();
        #[cfg(feature = "mini")]
        let actor_id = {
            use apx_sdk::core::crypto::eddsa::ed25519_public_key_from_secret_key;
            use apx_sdk::core::did_key::DidKey;
            let identity_public_key = ed25519_public_key_from_secret_key(&instance.ed25519_secret_key);
            let identity = DidKey::from_ed25519_key(&identity_public_key);
            format!("ap://{}/actors/{}", identity, user.id)
        };
        #[cfg(not(feature = "mini"))]
        let actor_id = local_actor_id(instance.uri_str(), &user.profile.username);

        let actor_key_id = local_actor_key_id(&actor_id, PublicKeyType::RsaPkcs1);
        (actor_key, actor_key_id)
    } else {
        let instance_actor_id = local_instance_actor_id(instance.uri_str());
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
        let instance_uri = "https://social.example";
        let instance = Instance::for_test(instance_uri);
        let agent = build_federation_agent(&instance, None);
        assert_eq!(agent.user_agent.is_none(), true);
        assert_eq!(agent.ssrf_protection_enabled, true);
        assert_eq!(agent.response_size_limit, RESPONSE_SIZE_LIMIT);
        assert_eq!(agent.signer.is_none(), true);
    }

    #[test]
    fn test_build_federation_agent() {
        let instance_uri = "https://social.example";
        let mut instance = Instance::for_test(instance_uri);
        instance.federation.enabled = true;
        let agent = build_federation_agent(&instance, None);
        assert_eq!(agent.user_agent.unwrap().ends_with(instance_uri), true);
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
