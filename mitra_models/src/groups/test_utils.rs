use apx_core::crypto::{
    eddsa::generate_weak_ed25519_key,
    rsa::generate_weak_rsa_key,
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
