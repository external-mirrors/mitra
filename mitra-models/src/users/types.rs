use std::collections::HashMap;

use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::Deserialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_utils::{
    crypto_eddsa::{
        ed25519_private_key_from_bytes,
        Ed25519PrivateKey,
    },
    crypto_rsa::{
        rsa_private_key_from_pkcs8_pem,
        RsaPrivateKey,
    },
    currencies::Currency,
    did::Did,
};

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    json_macro::json_from_sql,
    DatabaseTypeError,
};
use crate::profiles::types::DbActorProfile;

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "user_invite_code")]
pub struct DbInviteCode {
    pub code: String,
    used: bool,
    pub note: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(PartialEq)]
pub enum Permission {
    CreateFollowRequest,
    CreatePost,
    DeleteAnyPost,
    DeleteAnyProfile,
    ManageSubscriptionOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Guest,
    NormalUser,
    Admin,
    ReadOnlyUser,
}

impl Default for Role {
    fn default() -> Self { Self::NormalUser }
}

impl Role {
    pub fn get_permissions(&self) -> Vec<Permission> {
        match self {
            Self::Guest => vec![],
            Self::NormalUser => vec![
                Permission::CreateFollowRequest,
                Permission::CreatePost,
                Permission::ManageSubscriptionOptions,
            ],
            Self::Admin => vec![
                Permission::CreateFollowRequest,
                Permission::CreatePost,
                Permission::DeleteAnyPost,
                Permission::DeleteAnyProfile,
                Permission::ManageSubscriptionOptions,
            ],
            Self::ReadOnlyUser => vec![
                Permission::CreateFollowRequest,
            ],
        }
    }

    pub fn has_permission(&self, permission: Permission) -> bool {
        self.get_permissions().contains(&permission)
    }
}

impl From<&Role> for i16 {
    fn from(value: &Role) -> i16 {
        match value {
            Role::Guest => 0,
            Role::NormalUser => 1,
            Role::Admin => 2,
            Role::ReadOnlyUser => 3,
        }
    }
}

impl TryFrom<i16> for Role {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let role = match value {
            0 => Self::Guest,
            1 => Self::NormalUser,
            2 => Self::Admin,
            3 => Self::ReadOnlyUser,
            _ => return Err(DatabaseTypeError),
        };
        Ok(role)
    }
}

int_enum_from_sql!(Role);
int_enum_to_sql!(Role);

pub type ClientConfig = HashMap<String, JsonValue>;

#[derive(Deserialize)]
pub struct DbClientConfig(ClientConfig);

impl DbClientConfig {
    pub fn into_inner(self) -> ClientConfig {
        let Self(client_config) = self;
        client_config
    }
}

json_from_sql!(DbClientConfig);

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "user_account")]
pub struct DbUser {
    id: Uuid,
    password_hash: Option<String>,
    login_address_ethereum: Option<String>,
    login_address_monero: Option<String>,
    rsa_private_key: String,
    ed25519_private_key: Option<Vec<u8>>,
    invite_code: Option<String>,
    user_role: Role,
    client_config: DbClientConfig,
    created_at: DateTime<Utc>,
}

// Use wrapper because Ed25519PrivateKey doesn't implement Clone
pub struct DbEd25519PrivateKey(Ed25519PrivateKey);

impl DbEd25519PrivateKey {
    pub fn inner(&self) -> &Ed25519PrivateKey {
        let Self(private_key) = self;
        private_key
    }
}

impl Clone for DbEd25519PrivateKey {
    fn clone(&self) -> Self {
        let bytes = self.inner().as_bytes();
        let private_key = Ed25519PrivateKey::from_bytes(bytes)
            .expect("should be valid private key");
        Self(private_key)
    }
}

// Represents local user
#[derive(Clone)]
pub struct User {
    pub id: Uuid,
    pub password_hash: Option<String>,
    pub login_address_ethereum: Option<String>,
    pub login_address_monero: Option<String>,
    pub rsa_private_key: RsaPrivateKey,
    pub ed25519_private_key: Option<DbEd25519PrivateKey>,
    pub role: Role,
    pub client_config: ClientConfig,
    pub profile: DbActorProfile,
}

#[cfg(feature = "test-utils")]
impl Default for User {
    fn default() -> Self {
        use mitra_utils::crypto_rsa::generate_weak_rsa_key;
        Self {
            id: Default::default(),
            password_hash: None,
            login_address_ethereum: None,
            login_address_monero: None,
            rsa_private_key: generate_weak_rsa_key().unwrap(),
            ed25519_private_key: None,
            role: Role::default(),
            client_config: ClientConfig::default(),
            profile: DbActorProfile::default(),
        }
    }
}

impl User {
    pub fn new(
        db_user: DbUser,
        db_profile: DbActorProfile,
    ) -> Result<Self, DatabaseTypeError> {
        if db_user.id != db_profile.id {
            return Err(DatabaseTypeError);
        };
        let rsa_private_key =
            rsa_private_key_from_pkcs8_pem(&db_user.rsa_private_key)
                .map_err(|_| DatabaseTypeError)?;
        let maybe_ed25519_private_key = match db_user.ed25519_private_key {
            Some(ref bytes) => {
                let private_key = ed25519_private_key_from_bytes(bytes)
                    .map_err(|_| DatabaseTypeError)?;
                Some(DbEd25519PrivateKey(private_key))
            },
            None => None,
        };
        let user = Self {
            id: db_user.id,
            password_hash: db_user.password_hash,
            login_address_ethereum: db_user.login_address_ethereum,
            login_address_monero: db_user.login_address_monero,
            rsa_private_key: rsa_private_key,
            ed25519_private_key: maybe_ed25519_private_key,
            role: db_user.user_role,
            client_config: db_user.client_config.into_inner(),
            profile: db_profile,
        };
        Ok(user)
    }

    /// Returns wallet address if it is verified
    pub fn public_wallet_address(&self, currency: &Currency) -> Option<String> {
        for proof in self.profile.identity_proofs.clone().into_inner() {
            let did_pkh = match proof.issuer {
                Did::Pkh(did_pkh) => did_pkh,
                _ => continue,
            };
            // Return the first matching address, because only
            // one proof per currency is allowed.
            if let Some(ref address_currency) = did_pkh.currency() {
                if address_currency == currency {
                    return Some(did_pkh.address());
                };
            };
        };
        None
    }
}

pub struct UserCreateData {
    pub username: String,
    pub password_hash: Option<String>,
    pub login_address_ethereum: Option<String>,
    pub login_address_monero: Option<String>,
    pub rsa_private_key: String,
    pub invite_code: Option<String>,
    pub role: Role,
}

impl UserCreateData {
    pub(super) fn check_consistency(&self) -> Result<(), DatabaseTypeError> {
        if self.password_hash.is_none() &&
            self.login_address_ethereum.is_none() &&
            self.login_address_monero.is_none()
        {
            // At least one login method must be specified
            return Err(DatabaseTypeError);
        };
        Ok(())
    }
}

#[cfg(feature = "test-utils")]
impl Default for UserCreateData {
    fn default() -> Self {
        use mitra_utils::crypto_rsa::{
            generate_weak_rsa_key,
            rsa_private_key_to_pkcs8_pem,
        };
        let rsa_private_key = generate_weak_rsa_key().unwrap();
        let rsa_private_key_pem =
            rsa_private_key_to_pkcs8_pem(&rsa_private_key).unwrap();
        Self {
            username: Default::default(),
            password_hash: None,
            login_address_ethereum: None,
            login_address_monero: None,
            rsa_private_key: rsa_private_key_pem,
            invite_code: None,
            role: Role::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_wallet_address_login_address_not_exposed() {
        let user = User {
            login_address_ethereum: Some("0x1234".to_string()),
            ..Default::default()
        };
        let ethereum = Currency::Ethereum;
        assert_eq!(user.public_wallet_address(&ethereum), None);
    }
}
