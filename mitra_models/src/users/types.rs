use std::collections::HashMap;
use std::fmt;

use apx_core::{
    crypto_eddsa::{
        ed25519_secret_key_from_bytes,
        Ed25519SecretKey,
    },
    crypto_rsa::{
        rsa_secret_key_from_pkcs1_der,
        rsa_secret_key_from_pkcs8_pem,
        RsaSecretKey,
    },
    did::Did,
};
use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::Deserialize;
use serde_json::{Value as JsonValue};
use tokio_postgres::Row;
use uuid::Uuid;

use crate::{
    database::{
        int_enum::{int_enum_from_sql, int_enum_to_sql},
        json_macro::json_from_sql,
        DatabaseError,
        DatabaseTypeError,
    },
    profiles::types::{get_identity_key, DbActorProfile},
};

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

#[derive(Clone, Copy, Debug, PartialEq)]
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

impl From<Role> for i16 {
    fn from(value: Role) -> i16 {
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
    password_digest: Option<String>,
    login_address_ethereum: Option<String>,
    login_address_monero: Option<String>,
    rsa_private_key: String,
    ed25519_private_key: Vec<u8>,
    invite_code: Option<String>,
    user_role: Role,
    client_config: DbClientConfig,
    created_at: DateTime<Utc>,
}

// Represents local user (managed account)
#[derive(Clone)]
pub struct User {
    pub id: Uuid,
    pub password_digest: Option<String>,
    pub login_address_ethereum: Option<String>,
    pub login_address_monero: Option<String>,
    pub rsa_secret_key: RsaSecretKey,
    pub ed25519_secret_key: Ed25519SecretKey,
    pub role: Role,
    pub client_config: ClientConfig,
    pub profile: DbActorProfile,
}

impl fmt::Display for User {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.profile)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for User {
    fn default() -> Self {
        use apx_core::{
            crypto_eddsa::generate_weak_ed25519_key,
            crypto_rsa::generate_weak_rsa_key,
        };
        let id = Uuid::new_v4();
        Self {
            id: id,
            password_digest: None,
            login_address_ethereum: None,
            login_address_monero: None,
            rsa_secret_key: generate_weak_rsa_key().unwrap(),
            ed25519_secret_key: generate_weak_ed25519_key(),
            role: Role::default(),
            client_config: ClientConfig::default(),
            profile: DbActorProfile {
                id: id,
                ..Default::default()
            },
        }
    }
}

impl User {
    pub fn new(
        db_user: DbUser,
        db_profile: DbActorProfile,
    ) -> Result<Self, DatabaseTypeError> {
        db_profile.check_consistency()?;
        if !db_profile.is_local() {
            return Err(DatabaseTypeError);
        };
        if db_user.id != db_profile.id {
            return Err(DatabaseTypeError);
        };
        if db_profile.user_id != Some(db_user.id) {
            return Err(DatabaseTypeError);
        };
        let rsa_secret_key =
            rsa_secret_key_from_pkcs8_pem(&db_user.rsa_private_key)
                .map_err(|_| DatabaseTypeError)?;
        let ed25519_secret_key =
            ed25519_secret_key_from_bytes(&db_user.ed25519_private_key)
                .map_err(|_| DatabaseTypeError)?;
        if let Some(ref identity_key) = db_profile.identity_key {
            if *identity_key != get_identity_key(ed25519_secret_key) {
                return Err(DatabaseTypeError);
            };
        };
        let user = Self {
            id: db_user.id,
            password_digest: db_user.password_digest,
            login_address_ethereum: db_user.login_address_ethereum,
            login_address_monero: db_user.login_address_monero,
            rsa_secret_key: rsa_secret_key,
            ed25519_secret_key: ed25519_secret_key,
            role: db_user.user_role,
            client_config: db_user.client_config.into_inner(),
            profile: db_profile,
        };
        Ok(user)
    }

    /// Returns wallet address if it is verified
    pub fn public_ethereum_address(&self) -> Option<String> {
        for proof in self.profile.identity_proofs.clone().into_inner() {
            let did_pkh = match proof.issuer {
                Did::Pkh(did_pkh) => did_pkh,
                _ => continue,
            };
            // Return the first matching address, because only
            // one proof per currency is allowed.
            if did_pkh.chain_id().is_ethereum() {
                return Some(did_pkh.address());
            };
        };
        None
    }
}

pub struct UserCreateData {
    pub username: String,
    pub password_digest: Option<String>,
    pub login_address_ethereum: Option<String>,
    pub login_address_monero: Option<String>,
    pub rsa_secret_key: String,
    pub ed25519_secret_key: Ed25519SecretKey,
    pub invite_code: Option<String>,
    pub role: Role,
}

impl UserCreateData {
    pub(super) fn check_consistency(&self) -> Result<(), DatabaseTypeError> {
        if self.password_digest.is_none() &&
            self.login_address_ethereum.is_none() &&
            self.login_address_monero.is_none()
        {
            // At least one login method must be specified
            return Err(DatabaseTypeError);
        };
        Ok(())
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for UserCreateData {
    fn default() -> Self {
        use apx_core::{
            crypto_eddsa::generate_ed25519_key,
            crypto_rsa::{
                generate_weak_rsa_key,
                rsa_secret_key_to_pkcs8_pem,
            },
        };
        let rsa_secret_key = generate_weak_rsa_key().unwrap();
        let rsa_secret_key_pem =
            rsa_secret_key_to_pkcs8_pem(&rsa_secret_key).unwrap();
        // Generating unique key for each user to satisfy identity_key
        // uniqueness constraint.
        let ed25519_secret_key = generate_ed25519_key();
        Self {
            username: Default::default(),
            password_digest: None,
            login_address_ethereum: None,
            login_address_monero: None,
            rsa_secret_key: rsa_secret_key_pem,
            ed25519_secret_key: ed25519_secret_key,
            invite_code: None,
            role: Role::default(),
        }
    }
}

#[derive(FromSql)]
#[postgres(name = "portable_user_account")]
pub struct DbPortableUser {
    id: Uuid,
    rsa_secret_key: Vec<u8>,
    ed25519_secret_key: Vec<u8>,
    #[allow(dead_code)]
    invite_code: String,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
}

// Represents portable (remote) actor with local account (unmanaged)
pub struct PortableUser {
    pub id: Uuid,
    pub profile: DbActorProfile,
    pub rsa_secret_key: RsaSecretKey,
    pub ed25519_secret_key: Ed25519SecretKey,
}

impl PortableUser {
    pub fn new(
        db_user: DbPortableUser,
        db_profile: DbActorProfile,
    ) -> Result<Self, DatabaseTypeError> {
        db_profile.check_consistency()?;
        if !db_profile.is_portable() {
            return Err(DatabaseTypeError);
        };
        if db_user.id != db_profile.id {
            return Err(DatabaseTypeError);
        };
        if db_profile.portable_user_id != Some(db_user.id) {
            return Err(DatabaseTypeError);
        };
        let rsa_secret_key =
            rsa_secret_key_from_pkcs1_der(&db_user.rsa_secret_key)
                .map_err(|_| DatabaseTypeError)?;
        let ed25519_secret_key =
            ed25519_secret_key_from_bytes(&db_user.ed25519_secret_key)
                .map_err(|_| DatabaseTypeError)?;
        let user = Self {
            id: db_user.id,
            rsa_secret_key,
            ed25519_secret_key,
            profile: db_profile,
        };
        Ok(user)
    }
}

impl fmt::Display for PortableUser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.profile)
    }
}

pub struct PortableUserData {
    pub profile_id: Uuid,
    pub rsa_secret_key: RsaSecretKey,
    pub ed25519_secret_key: Ed25519SecretKey,
    pub invite_code: String,
}

pub struct AccountAdminInfo {
    pub profile: DbActorProfile,
    pub is_portable: bool,
    pub role: Option<Role>,
    pub last_login: Option<DateTime<Utc>>,
}

impl TryFrom<&Row> for AccountAdminInfo {

    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let profile = row.try_get("actor_profile")?;
        let is_portable = row.try_get("is_portable")?;
        let role = row.try_get("role")?;
        let last_login = row.try_get("last_login")?;
        let user = Self { profile, is_portable, role, last_login };
        user.profile.check_consistency()?;
        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use apx_core::crypto_eddsa::generate_ed25519_key;
    use super::*;

    #[test]
    fn test_user_cloned() {
        let ed25519_secret_key = generate_ed25519_key();
        let user = User {
            ed25519_secret_key: ed25519_secret_key,
            ..Default::default()
        };
        let user_cloned = user.clone();
        assert_eq!(
            user_cloned.ed25519_secret_key,
            ed25519_secret_key,
        );
    }

    #[test]
    fn test_public_ethereum_address_login_address_not_exposed() {
        let user = User {
            login_address_ethereum: Some("0x1234".to_string()),
            ..Default::default()
        };
        assert_eq!(user.public_ethereum_address(), None);
    }
}
