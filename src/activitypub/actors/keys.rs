use serde::{Deserialize, Serialize};

use mitra_models::profiles::types::{DbActorKey, PublicKeyType};
use mitra_utils::{
    crypto_rsa::{
        deserialize_rsa_public_key,
        rsa_public_key_from_pkcs1_der,
        rsa_public_key_to_pkcs1_der,
        rsa_public_key_to_pkcs8_pem,
        RsaPrivateKey,
        RsaPublicKey,
        RsaSerializationError,
    },
    multibase::{decode_multibase_base58btc, encode_multibase_base58btc},
    multicodec::{decode_rsa_public_key, encode_rsa_public_key},
};

use crate::activitypub::{
    identifiers::local_actor_key_id,
    vocabulary::MULTIKEY,
};
use crate::errors::ValidationError;

#[derive(Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct PublicKey {
    pub id: String,
    pub owner: String,
    pub public_key_pem: String,
}

impl PublicKey {
    pub fn build(
        actor_id: &str,
        private_key: &RsaPrivateKey,
    ) -> Result<Self, RsaSerializationError> {
        let public_key = RsaPublicKey::from(private_key);
        let public_key_pem = rsa_public_key_to_pkcs8_pem(&public_key)?;
        let public_key_obj = Self {
            id: local_actor_key_id(actor_id),
            owner: actor_id.to_string(),
            public_key_pem: public_key_pem,
        };
        Ok(public_key_obj)
    }

    pub fn to_db_key(&self) -> Result<DbActorKey, ValidationError> {
        let public_key = deserialize_rsa_public_key(&self.public_key_pem)
            .map_err(|_| ValidationError("invalid key encoding"))?;
        let public_key_der = rsa_public_key_to_pkcs1_der(&public_key)
            .map_err(|_| ValidationError("invalid public key"))?;
        let db_key = DbActorKey {
            id: self.id.clone(),
            key_type: PublicKeyType::RsaPkcs1,
            key_data: public_key_der,
        };
        Ok(db_key)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Multikey {
    id: String,
    #[serde(rename = "type")]
    object_type: String,
    pub controller: String,
    public_key_multibase: String,
}

impl Multikey {
    // FEP-521a
    pub fn build(
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

    pub fn to_db_key(&self) -> Result<DbActorKey, ValidationError> {
        let public_key_multicode = decode_multibase_base58btc(&self.public_key_multibase)
            .map_err(|_| ValidationError("invalid key encoding"))?;
        let public_key_der = decode_rsa_public_key(&public_key_multicode)
            .map_err(|_| ValidationError("unexpected key type"))?;
        rsa_public_key_from_pkcs1_der(&public_key_der)
            .map_err(|_| ValidationError("invalid key encoding"))?;
        let db_key = DbActorKey {
            id: self.id.clone(),
            key_type: PublicKeyType::RsaPkcs1,
            key_data: public_key_der,
        };
        Ok(db_key)
    }
}
