use serde::{Deserialize, Serialize};

use mitra_utils::{
    crypto_rsa::{
        rsa_public_key_to_pkcs1_der,
        rsa_public_key_to_pkcs8_pem,
        RsaPrivateKey,
        RsaPublicKey,
        RsaSerializationError,
    },
    multibase::encode_multibase_base58btc,
    multicodec::encode_rsa_public_key,
};

use crate::activitypub::{
    identifiers::local_actor_key_id,
    vocabulary::MULTIKEY,
};

#[derive(Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct PublicKey {
    pub id: String,
    pub owner: String,
    pub public_key_pem: String,
}

impl PublicKey {
    pub fn new(
        actor_id: &str,
        private_key: &RsaPrivateKey,
    ) -> Result<Self, RsaSerializationError> {
        let public_key = RsaPublicKey::from(private_key);
        let public_key_pem = rsa_public_key_to_pkcs8_pem(&public_key)?;
        let public_key = PublicKey {
            id: local_actor_key_id(actor_id),
            owner: actor_id.to_string(),
            public_key_pem: public_key_pem,
        };
        Ok(public_key)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Multikey {
    id: String,
    #[serde(rename = "type")]
    object_type: String,
    controller: String,
    public_key_multibase: String,
}

impl Multikey {
    // FEP-521a
    pub fn new(
        actor_id: &str,
        private_key: &RsaPrivateKey,
    ) -> Result<Self, RsaSerializationError> {
        let public_key = RsaPublicKey::from(private_key);
        let public_key_der = rsa_public_key_to_pkcs1_der(&public_key)?;
        let public_key_multicode = encode_rsa_public_key(&public_key_der);
        let public_key_multibase = encode_multibase_base58btc(&public_key_multicode);
        let multikey = Self {
            id: local_actor_key_id(actor_id),
            object_type: MULTIKEY.to_string(),
            controller: actor_id.to_string(),
            public_key_multibase,
        };
        Ok(multikey)
    }
}
