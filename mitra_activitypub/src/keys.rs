use apx_core::{
    crypto::{
        common::PublicKey,
        eddsa::{
            ed25519_public_key_from_secret_key,
            ed25519_public_key_to_multikey,
            Ed25519SecretKey,
        },
        rsa::{
            rsa_public_key_to_multikey,
            rsa_public_key_to_pkcs1_der,
            rsa_public_key_to_pkcs8_pem,
            RsaPublicKey,
            RsaSecretKey,
            RsaSerializationError,
        },
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use mitra_models::profiles::types::{DbActorKey, PublicKeyType};
use mitra_validators::{
    errors::ValidationError,
};

use crate::{
    identifiers::{canonicalize_id, local_actor_key_id},
    vocabulary::MULTIKEY,
};

fn to_db_key(key_id: &str, public_key: PublicKey) -> Result<DbActorKey, ValidationError> {
    let key_id = canonicalize_id(key_id)?;
    let (key_type, key_data) = match public_key {
        PublicKey::Rsa(public_key) => {
            let public_key_der = rsa_public_key_to_pkcs1_der(&public_key)
                .map_err(|_| ValidationError("invalid public key"))?;
            (PublicKeyType::RsaPkcs1, public_key_der)
        },
        PublicKey::Ed25519(public_key) => {
            (PublicKeyType::Ed25519, public_key.to_bytes().to_vec())
        },
    };
    let db_key = DbActorKey {
        id: key_id.to_string(),
        key_type,
        key_data,
    };
    Ok(db_key)
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyPem {
    pub id: String,
    pub owner: String,
    public_key_pem: String,
}

impl PublicKeyPem {
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

    pub fn public_key(&self) -> Result<PublicKey, ValidationError> {
        let public_key = PublicKey::from_pem(&self.public_key_pem)
            .map_err(ValidationError)?;
        Ok(public_key)
    }

    pub fn to_db_key(&self) -> Result<DbActorKey, ValidationError> {
        to_db_key(&self.id, self.public_key()?)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Multikey {
    pub id: String,
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

    pub fn public_key(&self) -> Result<PublicKey, ValidationError> {
        let public_key = PublicKey::from_multikey(&self.public_key_multibase)
            .map_err(ValidationError)?;
        Ok(public_key)
    }

    pub fn to_db_key(&self) -> Result<DbActorKey, ValidationError> {
        to_db_key(&self.id, self.public_key()?)
    }
}

pub fn verification_method_to_public_key(
    verification_method: JsonValue,
) -> Result<PublicKey, ValidationError> {
    if verification_method["type"].as_str() == Some(MULTIKEY) {
        let key: Multikey = serde_json::from_value(verification_method)
            .map_err(|_| ValidationError("invalid verification method"))?;
        key.public_key()
    } else {
        let key: PublicKeyPem = serde_json::from_value(verification_method)
            .map_err(|_| ValidationError("invalid verification method"))?;
        key.public_key()
    }
}

#[cfg(test)]
mod tests {
    use apx_core::{
        crypto::{
            eddsa::{
                ed25519_public_key_from_bytes,
                generate_ed25519_key,
            },
            rsa::{
                generate_weak_rsa_key,
                rsa_public_key_from_pkcs1_der,
            },
        },
    };
    use super::*;

    #[test]
    fn test_public_key_pem() {
        let actor_id = "https://test.example/users/1";
        let secret_key = generate_weak_rsa_key().unwrap();
        let public_key_pem = PublicKeyPem::build(actor_id, &secret_key).unwrap();
        assert_eq!(public_key_pem.id, "https://test.example/users/1#main-key");
        assert_eq!(public_key_pem.owner, actor_id);
        let db_key = public_key_pem.to_db_key().unwrap();
        assert_eq!(db_key.id, public_key_pem.id);
        assert_eq!(db_key.key_type, PublicKeyType::RsaPkcs1);
        let public_key = rsa_public_key_from_pkcs1_der(&db_key.key_data).unwrap();
        assert_eq!(public_key, RsaPublicKey::from(secret_key));
    }

    #[test]
    fn test_multikey_ed25519() {
        let actor_id = "https://test.example/users/1";
        let secret_key = generate_ed25519_key();
        let multikey = Multikey::build_ed25519(actor_id, &secret_key);
        assert_eq!(multikey.id, "https://test.example/users/1#ed25519-key");
        assert_eq!(multikey.controller, actor_id);
        let db_key = multikey.to_db_key().unwrap();
        assert_eq!(db_key.id, multikey.id);
        assert_eq!(db_key.key_type, PublicKeyType::Ed25519);
        let public_key = ed25519_public_key_from_bytes(&db_key.key_data).unwrap();
        assert_eq!(public_key, ed25519_public_key_from_secret_key(&secret_key));
    }
}
