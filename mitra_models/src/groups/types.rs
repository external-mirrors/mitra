use apx_core::crypto::{
    eddsa::Ed25519SecretKey,
    rsa::RsaSecretKey,
};
use uuid::Uuid;

#[derive(PartialEq)]
pub enum GroupType {
    Public,
    Private,
    // TODO: SemiPrivate,
    // TODO: postingRestrictedToMods
}

pub struct GroupCreateData {
    pub username: String,
    pub bio: Option<String>,
    pub bio_source: Option<String>,
    pub emojis: Vec<Uuid>,
    pub rsa_secret_key: RsaSecretKey,
    pub ed25519_secret_key: Ed25519SecretKey,
}

pub enum GroupFilter {
    Following,
    Moderating,
}
