use std::fmt;
use std::num::NonZeroU64;

use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeserializerError,
    ser::SerializeMap,
    __private::ser::FlatMapSerializer,
};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_utils::{
    caip2::ChainId,
    crypto_eddsa::{
        ed25519_public_key_from_private_key,
        Ed25519PrivateKey,
    },
    did::Did,
    did_key::DidKey,
};

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    json_macro::{json_from_sql, json_to_sql},
    DatabaseTypeError,
};
use crate::emojis::types::DbEmoji;

use super::checks::{
    check_identity_proofs,
    check_payment_options,
    check_public_keys,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProfileImage {
    pub file_name: String,
    pub file_size: Option<usize>,
    pub media_type: Option<String>,
}

impl ProfileImage {
    pub fn new(
        file_name: String,
        file_size: usize,
        media_type: String,
    ) -> Self {
        Self {
            file_name,
            file_size: Some(file_size),
            media_type: Some(media_type),
        }
    }
}

json_from_sql!(ProfileImage);
json_to_sql!(ProfileImage);

#[derive(Clone, Debug, Default)]
pub enum MentionPolicy {
    #[default]
    None,
    OnlyKnown,
    OnlyContacts,
}

impl From<&MentionPolicy> for i16 {
    fn from(value: &MentionPolicy) -> i16 {
        match value {
            MentionPolicy::None => 0,
            MentionPolicy::OnlyKnown => 1,
            MentionPolicy::OnlyContacts => 2,
        }
    }
}

impl TryFrom<i16> for MentionPolicy {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let policy = match value {
            0 => Self::None,
            1 => Self::OnlyKnown,
            2 => Self::OnlyContacts,
            _ => return Err(DatabaseTypeError),
        };
        Ok(policy)
    }
}

int_enum_from_sql!(MentionPolicy);
int_enum_to_sql!(MentionPolicy);

#[derive(Clone, Debug, PartialEq)]
pub enum PublicKeyType {
    RsaPkcs1,
    Ed25519,
}

impl From<&PublicKeyType> for i16 {
    fn from(key_type: &PublicKeyType) -> i16 {
        match key_type {
            PublicKeyType::RsaPkcs1 => 1,
            PublicKeyType::Ed25519 => 2,
        }
    }
}

impl TryFrom<i16> for PublicKeyType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let key_type = match value {
            1 => Self::RsaPkcs1,
            2 => Self::Ed25519,
            _ => return Err(DatabaseTypeError),
        };
        Ok(key_type)
    }
}

impl<'de> Deserialize<'de> for PublicKeyType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        i16::deserialize(deserializer)?
            .try_into().map_err(DeserializerError::custom)
    }
}

impl Serialize for PublicKeyType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        serializer.serialize_i16(self.into())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DbActorKey {
    pub id: String,
    pub key_type: PublicKeyType,
    #[serde(with = "hex::serde")]
    pub key_data: Vec<u8>,
}

#[cfg(feature = "test-utils")]
impl Default for DbActorKey {
    fn default() -> Self {
        Self {
            id: Default::default(),
            key_type: PublicKeyType::RsaPkcs1,
            key_data: vec![],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicKeys(pub Vec<DbActorKey>);

impl PublicKeys {
    pub fn inner(&self) -> &[DbActorKey] {
        let Self(public_keys) = self;
        public_keys
    }

    pub fn into_inner(self) -> Vec<DbActorKey> {
        let Self(public_keys) = self;
        public_keys
    }
}

json_from_sql!(PublicKeys);
json_to_sql!(PublicKeys);

#[derive(Clone, Debug, PartialEq)]
pub enum IdentityProofType {
    LegacyEip191IdentityProof,
    LegacyMinisignIdentityProof,
    FepC390JcsBlake2Ed25519Proof, // MitraJcsEd25519Signature2022
    FepC390JcsEip191Proof, // MitraJcsEip191Signature2022
    FepC390LegacyJcsEddsaProof, // jcs-eddsa-2022
    FepC390EddsaJcsNoCiProof, // was used for incorrect eddsa-jcs-2022 proofs
}

impl IdentityProofType {
    pub fn is_legacy(&self) -> bool {
        // Mitra 1.x identity proofs
        matches!(
            self,
            Self::LegacyEip191IdentityProof | Self::LegacyMinisignIdentityProof,
        )
    }
}

impl From<&IdentityProofType> for i16 {
    fn from(proof_type: &IdentityProofType) -> i16 {
        match proof_type {
            IdentityProofType::LegacyEip191IdentityProof => 1,
            IdentityProofType::LegacyMinisignIdentityProof => 2,
            IdentityProofType::FepC390JcsBlake2Ed25519Proof => 3,
            IdentityProofType::FepC390JcsEip191Proof => 4,
            IdentityProofType::FepC390LegacyJcsEddsaProof => 5,
            IdentityProofType::FepC390EddsaJcsNoCiProof => 6,
        }
    }
}

impl TryFrom<i16> for IdentityProofType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let proof_type = match value {
            1 => Self::LegacyEip191IdentityProof,
            2 => Self::LegacyMinisignIdentityProof,
            3 => Self::FepC390JcsBlake2Ed25519Proof,
            4 => Self::FepC390JcsEip191Proof,
            5 => Self::FepC390LegacyJcsEddsaProof,
            6 => Self::FepC390EddsaJcsNoCiProof,
            _ => return Err(DatabaseTypeError),
        };
        Ok(proof_type)
    }
}

impl<'de> Deserialize<'de> for IdentityProofType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        i16::deserialize(deserializer)?
            .try_into().map_err(DeserializerError::custom)
    }
}

impl Serialize for IdentityProofType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        serializer.serialize_i16(self.into())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdentityProof {
    pub issuer: Did,
    pub proof_type: IdentityProofType,
    pub value: JsonValue,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdentityProofs(pub Vec<IdentityProof>);

impl IdentityProofs {
    pub fn inner(&self) -> &[IdentityProof] {
        let Self(identity_proofs) = self;
        identity_proofs
    }

    pub fn into_inner(self) -> Vec<IdentityProof> {
        let Self(identity_proofs) = self;
        identity_proofs
    }

    /// Returns true if identity proof list contains at least one proof
    /// created by a given DID.
    pub fn any(&self, issuer: &Did) -> bool {
        let Self(identity_proofs) = self;
        identity_proofs.iter().any(|proof| proof.issuer == *issuer)
    }
}

json_from_sql!(IdentityProofs);
json_to_sql!(IdentityProofs);

#[derive(PartialEq)]
pub enum PaymentType {
    Link,
    EthereumSubscription,
    MoneroSubscription,
    RemoteMoneroSubscription,
}

impl From<&PaymentType> for i16 {
    fn from(payment_type: &PaymentType) -> i16 {
        match payment_type {
            PaymentType::Link => 1,
            PaymentType::EthereumSubscription => 2,
            PaymentType::MoneroSubscription => 3,
            PaymentType::RemoteMoneroSubscription => 4,
        }
    }
}

impl TryFrom<i16> for PaymentType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let payment_type = match value {
            1 => Self::Link,
            2 => Self::EthereumSubscription,
            3 => Self::MoneroSubscription,
            4 => Self::RemoteMoneroSubscription,
            _ => return Err(DatabaseTypeError),
        };
        Ok(payment_type)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentLink {
    pub name: String,
    pub href: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EthereumSubscription {
    pub chain_id: ChainId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MoneroSubscription {
    pub chain_id: ChainId,
    pub price: NonZeroU64, // piconeros per second
    pub payout_address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RemoteMoneroSubscription {
    pub chain_id: ChainId,
    pub price: NonZeroU64, // piconeros per second
    pub object_id: String,
    #[serde(default)]
    pub fep_0837_enabled: bool,
}

#[derive(Clone, Debug)]
pub enum PaymentOption {
    Link(PaymentLink),
    EthereumSubscription(EthereumSubscription),
    MoneroSubscription(MoneroSubscription),
    RemoteMoneroSubscription(RemoteMoneroSubscription),
}

pub trait SubscriptionOption {
    fn chain_id(&self) -> ChainId;

    fn from_payment_option(option: &PaymentOption) -> Option<Self>
        where Self: Sized;
}

impl SubscriptionOption for MoneroSubscription {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn from_payment_option(option: &PaymentOption) -> Option<Self> {
        match option {
            PaymentOption::MoneroSubscription(info) => Some(info.clone()),
            _ => None,
        }
    }
}

impl SubscriptionOption for RemoteMoneroSubscription {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn from_payment_option(option: &PaymentOption) -> Option<Self> {
        match option {
            PaymentOption::RemoteMoneroSubscription(info) => Some(info.clone()),
            _ => None,
        }
    }
}

impl PaymentOption {
    pub fn ethereum_subscription(chain_id: ChainId) -> Self {
        Self::EthereumSubscription(EthereumSubscription { chain_id })
    }

    pub fn monero_subscription(
        chain_id: ChainId,
        price: NonZeroU64,
        payout_address: String,
    ) -> Self {
        Self::MoneroSubscription(MoneroSubscription {
            chain_id,
            price,
            payout_address,
        })
    }

    pub fn remote_monero_subscription(
        chain_id: ChainId,
        price: NonZeroU64,
        object_id: String,
        fep_0837_enabled: bool,
    ) -> Self {
        Self::RemoteMoneroSubscription(RemoteMoneroSubscription {
            chain_id,
            price,
            object_id,
            fep_0837_enabled,
        })
    }

    pub(super) fn payment_type(&self) -> PaymentType {
        match self {
            Self::Link(_) => PaymentType::Link,
            Self::EthereumSubscription(_) => PaymentType::EthereumSubscription,
            Self::MoneroSubscription(_) => PaymentType::MoneroSubscription,
            Self::RemoteMoneroSubscription(_) => PaymentType::RemoteMoneroSubscription,
        }
    }

    pub fn chain_id(&self) -> Option<&ChainId> {
        match self {
            Self::Link(_) => None,
            Self::EthereumSubscription(info) => Some(&info.chain_id),
            Self::MoneroSubscription(info) => Some(&info.chain_id),
            Self::RemoteMoneroSubscription(info) => Some(&info.chain_id),
        }
    }

    pub(super) fn check_chain_id(&self) -> Result<(), DatabaseTypeError> {
        match self {
            Self::Link(_) => (),
            Self::EthereumSubscription(payment_info) => {
                if !payment_info.chain_id.is_ethereum() {
                    return Err(DatabaseTypeError);
                };
            },
            Self::MoneroSubscription(payment_info) => {
                if !payment_info.chain_id.is_monero() {
                    return Err(DatabaseTypeError);
                };
            },
            Self::RemoteMoneroSubscription(payment_info) => {
                if !payment_info.chain_id.is_monero() {
                    return Err(DatabaseTypeError);
                };
            },
        };
        Ok(())
    }
}

// Integer tags are not supported https://github.com/serde-rs/serde/issues/745
// Workaround: https://stackoverflow.com/a/65576570
impl<'de> Deserialize<'de> for PaymentOption {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let value = JsonValue::deserialize(deserializer)?;
        let payment_type = value.get("payment_type")
            .and_then(JsonValue::as_u64)
            .and_then(|val| i16::try_from(val).ok())
            .and_then(|val| PaymentType::try_from(val).ok())
            .ok_or(DeserializerError::custom("invalid payment type"))?;
        let payment_option = match payment_type {
            PaymentType::Link => {
                let link = PaymentLink::deserialize(value)
                    .map_err(DeserializerError::custom)?;
                Self::Link(link)
            },
            PaymentType::EthereumSubscription => {
                let payment_info = EthereumSubscription::deserialize(value)
                    .map_err(DeserializerError::custom)?;
                Self::EthereumSubscription(payment_info)
            },
            PaymentType::MoneroSubscription => {
                let payment_info = MoneroSubscription::deserialize(value)
                    .map_err(DeserializerError::custom)?;
                Self::MoneroSubscription(payment_info)
            },
            PaymentType::RemoteMoneroSubscription => {
                let payment_info = RemoteMoneroSubscription::deserialize(value)
                    .map_err(DeserializerError::custom)?;
                Self::RemoteMoneroSubscription(payment_info)
            },
        };
        payment_option.check_chain_id().map_err(DeserializerError::custom)?;
        Ok(payment_option)
    }
}

impl Serialize for PaymentOption {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        let payment_type = self.payment_type();
        map.serialize_entry("payment_type", &i16::from(&payment_type))?;

        match self {
            Self::Link(link) => link.serialize(FlatMapSerializer(&mut map))?,
            Self::EthereumSubscription(payment_info) => {
                payment_info.serialize(FlatMapSerializer(&mut map))?
            },
            Self::MoneroSubscription(payment_info) => {
                payment_info.serialize(FlatMapSerializer(&mut map))?
            },
            Self::RemoteMoneroSubscription(payment_info) => {
                payment_info.serialize(FlatMapSerializer(&mut map))?
            },
        };
        map.end()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentOptions(pub Vec<PaymentOption>);

impl PaymentOptions {
    pub fn inner(&self) -> &[PaymentOption] {
        let Self(payment_options) = self;
        payment_options
    }

    pub fn into_inner(self) -> Vec<PaymentOption> {
        let Self(payment_options) = self;
        payment_options
    }

    /// Returns true if payment option list contains at least one option
    /// of the given type.
    pub fn any(&self, payment_type: PaymentType) -> bool {
        let Self(payment_options) = self;
        payment_options.iter()
            .any(|option| option.payment_type() == payment_type)
    }

    pub fn find_subscription_option<S: SubscriptionOption>(
        &self,
        chain_id: &ChainId,
    ) -> Option<S> {
        self.inner().iter()
            .filter_map(S::from_payment_option)
            .find(|payment_info| payment_info.chain_id() == *chain_id)
    }
}

json_from_sql!(PaymentOptions);
json_to_sql!(PaymentOptions);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraField {
    pub name: String,
    pub value: String,
    pub value_source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraFields(pub Vec<ExtraField>);

impl ExtraFields {
    pub fn into_inner(self) -> Vec<ExtraField> {
        let Self(extra_fields) = self;
        extra_fields
    }
}

json_from_sql!(ExtraFields);
json_to_sql!(ExtraFields);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Alias {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Aliases(Vec<Alias>);

impl Aliases {
    pub fn new(actor_ids: Vec<String>) -> Self {
        // Not signed
        let aliases = actor_ids.into_iter()
            .map(|actor_id| Alias { id: actor_id })
            .collect();
        Self(aliases)
    }

    pub fn into_actor_ids(self) -> Vec<String> {
        let Self(aliases) = self;
        aliases.into_iter().map(|alias| alias.id).collect()
    }
}

json_from_sql!(Aliases);
json_to_sql!(Aliases);

#[derive(Clone, Deserialize)]
pub struct ProfileEmojis(Vec<DbEmoji>);

impl ProfileEmojis {
    pub fn into_inner(self) -> Vec<DbEmoji> {
        let Self(emojis) = self;
        emojis
    }
}

json_from_sql!(ProfileEmojis);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "test-utils", derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct DbActorPublicKey {
    pub id: String,
    pub owner: String,
    pub public_key_pem: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "test-utils", derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct DbActor {
    #[serde(rename = "type")]
    pub object_type: String,

    pub id: String,
    pub inbox: String,
    pub outbox: String,
    pub followers: Option<String>,
    pub subscribers: Option<String>,
    pub featured: Option<String>,
    pub url: Option<String>,

    pub public_key: DbActorPublicKey,
}

json_from_sql!(DbActor);
json_to_sql!(DbActor);

#[derive(Clone, FromSql)]
#[postgres(name = "actor_profile")]
pub struct DbActorProfile {
    pub id: Uuid,
    pub username: String,
    pub hostname: Option<String>,
    pub acct: Option<String>, // unique acct string
    pub display_name: Option<String>,
    pub bio: Option<String>, // html
    pub bio_source: Option<String>, // plaintext or markdown
    pub avatar: Option<ProfileImage>,
    pub banner: Option<ProfileImage>,
    pub manually_approves_followers: bool,
    pub mention_policy: MentionPolicy,
    pub public_keys: PublicKeys,
    pub identity_proofs: IdentityProofs,
    pub payment_options: PaymentOptions,
    pub extra_fields: ExtraFields,
    pub aliases: Aliases,
    pub follower_count: i32,
    pub following_count: i32,
    pub subscriber_count: i32,
    pub post_count: i32,
    pub emojis: ProfileEmojis,
    pub actor_json: Option<DbActor>,
    pub identity_key: Option<String>, // multibase + multicodec
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub unreachable_since: Option<DateTime<Utc>>,

    // auto-generated database fields
    pub actor_id: Option<String>,
}

// Profile identifiers:
// id (local profile UUID): never changes
// actor_id of remote actor: must not change
// acct (webfinger): may change if actor ID remains the same
// actor RSA key: can be updated at any time by the instance admin
// identity proofs: TBD (likely will do "Trust on first use" (TOFU))

pub(super) fn get_profile_acct(username: &str, hostname: Option<&str>) -> String {
    if let Some(hostname) = hostname {
        format!("{}@{}", username, hostname)
    } else {
        username.to_owned()
    }
}

pub(crate) fn get_identity_key(secret_key: Ed25519PrivateKey) -> String {
    let public_key = ed25519_public_key_from_private_key(&secret_key);
    let did_key = DidKey::from_ed25519_key(&public_key);
    did_key.key_multibase()
}

impl DbActorProfile {
    pub(crate) fn check_consistency(&self) -> Result<(), DatabaseTypeError> {
        if self.hostname.is_none() != self.actor_json.is_none() {
            return Err(DatabaseTypeError);
        };
        if let Some(ref acct) = self.acct {
            let expected_acct = get_profile_acct(
                &self.username,
                self.hostname.as_deref(),
            );
            if acct != &expected_acct {
                return Err(DatabaseTypeError);
            };
        } else if self.hostname.is_none() {
            // Only remote accounts may have empty acct
            return Err(DatabaseTypeError);
        };
        if self.hostname.is_some() && self.identity_key.is_some() {
            // Remote accounts can't have identity keys
            return Err(DatabaseTypeError);
        };
        Ok(())
    }

    pub fn is_local(&self) -> bool {
        self.actor_json.is_none()
    }

    pub fn is_automated(&self) -> bool {
        match self.actor_json {
            Some(ref db_actor) => {
                ["Service", "Application"]
                    .contains(&db_actor.object_type.as_str())
            },
            None => false,
        }
    }

    fn expect_actor_data(&self) -> &DbActor {
        self.actor_json.as_ref()
            .expect("actor data should be present")
    }

    pub fn expect_remote_actor_id(&self) -> &str {
        &self.expect_actor_data().id
    }

    // For Mastodon API
    pub fn preferred_handle(&self) -> &str {
        if let Some(ref acct) = self.acct {
            acct
        } else {
            // Only remote actors may have empty acct
            self.expect_remote_actor_id()
        }
    }

    pub fn monero_subscription(
        &self,
        chain_id: &ChainId,
    ) -> Option<MoneroSubscription> {
        assert!(chain_id.is_monero());
        self.payment_options.find_subscription_option(chain_id)
    }
}

impl fmt::Display for DbActorProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref actor_json) = self.actor_json {
            write!(formatter, "{}", actor_json.id)
        } else {
            write!(formatter, "@{}", self.username)
        }
    }
}

#[cfg(feature = "test-utils")]
impl Default for DbActorProfile {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            username: "test".to_string(),
            hostname: None,
            acct: Some("test".to_string()),
            display_name: None,
            bio: None,
            bio_source: None,
            avatar: None,
            banner: None,
            manually_approves_followers: false,
            mention_policy: MentionPolicy::default(),
            public_keys: PublicKeys(vec![]),
            identity_proofs: IdentityProofs(vec![]),
            payment_options: PaymentOptions(vec![]),
            extra_fields: ExtraFields(vec![]),
            aliases: Aliases(vec![]),
            follower_count: 0,
            following_count: 0,
            subscriber_count: 0,
            post_count: 0,
            emojis: ProfileEmojis(vec![]),
            actor_json: None,
            actor_id: None,
            identity_key: None,
            created_at: now,
            updated_at: now,
            unreachable_since: None,
        }
    }
}

#[cfg_attr(feature = "test-utils", derive(Default))]
pub struct ProfileCreateData {
    pub username: String,
    pub hostname: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<ProfileImage>,
    pub banner: Option<ProfileImage>,
    pub manually_approves_followers: bool,
    pub mention_policy: MentionPolicy,
    pub public_keys: Vec<DbActorKey>,
    pub identity_proofs: Vec<IdentityProof>,
    pub payment_options: Vec<PaymentOption>,
    pub extra_fields: Vec<ExtraField>,
    pub aliases: Vec<String>,
    pub emojis: Vec<Uuid>,
    pub actor_json: Option<DbActor>,
}

impl ProfileCreateData {
    pub(super) fn check_consistency(&self) -> Result<(), DatabaseTypeError> {
        let is_remote = self.actor_json.is_some();
        check_public_keys(&self.public_keys, is_remote)?;
        check_identity_proofs(&self.identity_proofs)?;
        check_payment_options(&self.payment_options, is_remote)?;
        // Aliases are not checked.
        // The list may contain duplicates or self-references.
        Ok(())
    }
}

pub struct ProfileUpdateData {
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub bio_source: Option<String>,
    pub avatar: Option<ProfileImage>,
    pub banner: Option<ProfileImage>,
    pub manually_approves_followers: bool,
    pub mention_policy: MentionPolicy,
    pub public_keys: Vec<DbActorKey>,
    pub identity_proofs: Vec<IdentityProof>,
    pub payment_options: Vec<PaymentOption>,
    pub extra_fields: Vec<ExtraField>,
    pub aliases: Vec<String>,
    pub emojis: Vec<Uuid>,
    pub actor_json: Option<DbActor>,
}

impl ProfileUpdateData {
    pub(super) fn check_consistency(&self) -> Result<(), DatabaseTypeError> {
        let is_remote = self.actor_json.is_some();
        check_public_keys(&self.public_keys, is_remote)?;
        check_identity_proofs(&self.identity_proofs)?;
        check_payment_options(&self.payment_options, is_remote)?;
        Ok(())
    }

    /// Adds new identity proof
    /// or replaces the existing one if it has the same issuer.
    pub fn add_identity_proof(&mut self, proof: IdentityProof) -> () {
        self.identity_proofs.retain(|item| item.issuer != proof.issuer);
        self.identity_proofs.push(proof);
    }

    /// Adds new payment option
    /// or replaces the existing one if it has the same type.
    pub fn add_payment_option(&mut self, option: PaymentOption) -> () {
        self.payment_options.retain(|item| {
            item.payment_type() != option.payment_type()
        });
        self.payment_options.push(option);
    }
}

impl From<&DbActorProfile> for ProfileUpdateData {
    fn from(profile: &DbActorProfile) -> Self {
        let profile = profile.clone();
        Self {
            username: profile.username,
            display_name: profile.display_name,
            bio: profile.bio,
            bio_source: profile.bio_source,
            avatar: profile.avatar,
            banner: profile.banner,
            manually_approves_followers: profile.manually_approves_followers,
            mention_policy: profile.mention_policy,
            public_keys: profile.public_keys.into_inner(),
            identity_proofs: profile.identity_proofs.into_inner(),
            payment_options: profile.payment_options.into_inner(),
            extra_fields: profile.extra_fields.into_inner(),
            aliases: profile.aliases.into_actor_ids(),
            emojis: profile.emojis.into_inner().into_iter()
                .map(|emoji| emoji.id)
                .collect(),
            actor_json: profile.actor_json,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_key_serialization() {
        let json_data = r#"{"id":"https://test.example/keys/1","key_type":1,"key_data":"010203"}"#;
        let actor_key: DbActorKey = serde_json::from_str(json_data).unwrap();
        assert_eq!(actor_key.id, "https://test.example/keys/1");
        assert_eq!(actor_key.key_type, PublicKeyType::RsaPkcs1);
        assert_eq!(actor_key.key_data, vec![1, 2, 3]);
        let serialized = serde_json::to_string(&actor_key).unwrap();
        assert_eq!(serialized, json_data);
    }

    #[test]
    fn test_identity_proof_serialization() {
        let json_data = r#"{"issuer":"did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a","proof_type":1,"value":"dbfe"}"#;
        let proof: IdentityProof = serde_json::from_str(json_data).unwrap();
        let did_pkh = match proof.issuer {
            Did::Pkh(ref did_pkh) => did_pkh,
            _ => panic!("unexpected did method"),
        };
        assert_eq!(
            did_pkh.address(),
            "0xb9c5714089478a327f09197987f16f9e5d936e8a",
        );
        let serialized = serde_json::to_string(&proof).unwrap();
        assert_eq!(serialized, json_data);
    }

    #[test]
    fn test_payment_option_link_serialization() {
        let json_data = r#"{"payment_type":1,"name":"test","href":"https://test.com"}"#;
        let payment_option: PaymentOption = serde_json::from_str(json_data).unwrap();
        let link = match payment_option {
            PaymentOption::Link(ref link) => link,
            _ => panic!("wrong option"),
        };
        assert_eq!(link.name, "test");
        assert_eq!(link.href, "https://test.com");
        let serialized = serde_json::to_string(&payment_option).unwrap();
        assert_eq!(serialized, json_data);
    }

    #[test]
    fn test_payment_option_ethereum_subscription_serialization() {
        let json_data = r#"{"payment_type":2,"chain_id":"eip155:1","name":null}"#;
        let payment_option: PaymentOption = serde_json::from_str(json_data).unwrap();
        let payment_info = match payment_option {
            PaymentOption::EthereumSubscription(ref payment_info) => payment_info,
            _ => panic!("wrong option"),
        };
        assert_eq!(payment_info.chain_id, ChainId::ethereum_mainnet());
        let serialized = serde_json::to_string(&payment_option).unwrap();
        assert_eq!(serialized, r#"{"payment_type":2,"chain_id":"eip155:1"}"#);
    }

    #[test]
    fn test_alias() {
        let actor_id = "https://example.com/users/alice";
        let aliases = Aliases::new(vec![actor_id.to_string()]);
        let actor_ids = aliases.into_actor_ids();
        assert_eq!(actor_ids.len(), 1);
        assert_eq!(actor_ids[0], actor_id);
    }
}
