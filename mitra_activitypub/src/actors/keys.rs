use serde::{Deserialize, Serialize};

use mitra_models::profiles::types::{DbActorKey, PublicKeyType};
use mitra_utils::{
    crypto_eddsa::{
        ed25519_public_key_from_bytes,
        ed25519_public_key_from_pkcs8_pem,
        ed25519_public_key_from_secret_key,
        ed25519_public_key_to_multikey,
        Ed25519SecretKey,
    },
    crypto_rsa::{
        deserialize_rsa_public_key,
        rsa_public_key_from_pkcs1_der,
        rsa_public_key_to_multikey,
        rsa_public_key_to_pkcs1_der,
        rsa_public_key_to_pkcs8_pem,
        RsaSecretKey,
        RsaPublicKey,
        RsaSerializationError,
    },
    multibase::decode_multibase_base58btc,
    multicodec::Multicodec,
};
use mitra_validators::{
    errors::ValidationError,
};

use crate::{
    identifiers::local_actor_key_id,
    url::canonicalize_id,
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
    pub fn build(
        actor_id: &str,
        secret_key: &RsaSecretKey,
    ) -> Result<Self, RsaSerializationError> {
        let public_key = RsaPublicKey::from(secret_key);
        let public_key_pem = rsa_public_key_to_pkcs8_pem(&public_key)?;
        let public_key_obj = Self {
            id: local_actor_key_id(actor_id, PublicKeyType::RsaPkcs1),
            owner: actor_id.to_string(),
            public_key_pem: public_key_pem,
        };
        Ok(public_key_obj)
    }

    pub fn to_db_key(&self) -> Result<DbActorKey, ValidationError> {
        let key_id = canonicalize_id(&self.id)?;
        let (key_type, key_data) = match deserialize_rsa_public_key(&self.public_key_pem) {
            Ok(public_key) => {
                let public_key_der = rsa_public_key_to_pkcs1_der(&public_key)
                    .map_err(|_| ValidationError("invalid public key"))?;
                (PublicKeyType::RsaPkcs1, public_key_der)
            },
            Err(_) => {
                let public_key = ed25519_public_key_from_pkcs8_pem(&self.public_key_pem)
                    .map_err(|_| ValidationError("unexpected key type"))?;
                (PublicKeyType::Ed25519, public_key.to_bytes().to_vec())
            },
        };
        let db_key = DbActorKey {
            id: key_id,
            key_type,
            key_data,
        };
        Ok(db_key)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Multikey {
    id: String,
    #[serde(rename = "type")]
    object_type: String,
    pub controller: String,
    public_key_multibase: String,
}

// FEP-521a
impl Multikey {
    pub fn build_ed25519(
        actor_id: &str,
        secret_key: &Ed25519SecretKey,
    ) -> Self {
        let public_key = ed25519_public_key_from_secret_key(secret_key);
        let public_key_multibase = ed25519_public_key_to_multikey(&public_key);
        Self {
            id: local_actor_key_id(actor_id, PublicKeyType::Ed25519),
            object_type: MULTIKEY.to_string(),
            controller: actor_id.to_string(),
            public_key_multibase,
        }
    }

    pub fn build_rsa(
        actor_id: &str,
        secret_key: &RsaSecretKey,
    ) -> Result<Self, RsaSerializationError> {
        let public_key = RsaPublicKey::from(secret_key);
        let public_key_multibase = rsa_public_key_to_multikey(&public_key)?;
        let multikey = Self {
            id: local_actor_key_id(actor_id, PublicKeyType::RsaPkcs1),
            object_type: MULTIKEY.to_string(),
            controller: actor_id.to_string(),
            public_key_multibase,
        };
        Ok(multikey)
    }

    pub fn to_db_key(&self) -> Result<DbActorKey, ValidationError> {
        let key_id = canonicalize_id(&self.id)?;
        let public_key_multicode = decode_multibase_base58btc(&self.public_key_multibase)
            .map_err(|_| ValidationError("invalid key encoding"))?;
        let public_key_decoded = Multicodec::decode(&public_key_multicode)
            .map_err(|_| ValidationError("unexpected key type"))?;
        let (key_type, key_data) = match public_key_decoded {
            (Multicodec::RsaPub, public_key_der) => {
                // Validate RSA key
                rsa_public_key_from_pkcs1_der(&public_key_der)
                    .map_err(|_| ValidationError("invalid key encoding"))?;
                (PublicKeyType::RsaPkcs1, public_key_der)
            },
            (Multicodec::Ed25519Pub, public_key_bytes) => {
                // Validate Ed25519 key
                ed25519_public_key_from_bytes(&public_key_bytes)
                    .map_err(|_| ValidationError("invalid key encoding"))?;
                (PublicKeyType::Ed25519, public_key_bytes)
            },
            _ => return Err(ValidationError("unexpected key type")),
        };
        let db_key = DbActorKey {
            id: key_id,
            key_type,
            key_data,
        };
        Ok(db_key)
    }
}

#[cfg(test)]
mod tests {
    use mitra_utils::crypto_eddsa::generate_ed25519_key;
    use super::*;

    #[test]
    fn test_build_ed25519_multikey() {
        let actor_id = "https://test.example/users/1";
        let secret_key = generate_ed25519_key();
        let multikey = Multikey::build_ed25519(actor_id, &secret_key);
        assert_eq!(multikey.id, "https://test.example/users/1#ed25519-key");
        assert_eq!(multikey.controller, actor_id);
    }
}
