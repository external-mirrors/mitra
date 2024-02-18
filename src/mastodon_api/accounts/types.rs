use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_models::{
    profiles::types::{
        DbActorProfile,
        ExtraField,
        MentionPolicy,
        PaymentOption,
        ProfileImage,
        ProfileUpdateData,
    },
    subscriptions::types::Subscription,
    users::types::{
        ClientConfig,
        Permission,
        Role,
        User,
    },
};
use mitra_services::media::{get_file_url, MediaStorage};
use mitra_utils::{
    caip2::ChainId,
    did::Did,
    markdown::markdown_basic_to_html,
};
use mitra_validators::{
    errors::ValidationError,
    profiles::{allowed_profile_image_media_types, PROFILE_IMAGE_SIZE_MAX},
};

use crate::activitypub::identifiers::{
    profile_actor_id,
    profile_actor_url,
};
use crate::mastodon_api::{
    custom_emojis::types::CustomEmoji,
    errors::MastodonError,
    pagination::PageSize,
    uploads::{save_b64_file, UploadError},
};

pub const AUTHENTICATION_METHOD_PASSWORD: &str = "password";
pub const AUTHENTICATION_METHOD_EIP4361: &str = "eip4361";
pub const AUTHENTICATION_METHOD_CAIP122_MONERO: &str = "caip122_monero";

/// https://docs.joinmastodon.org/entities/field/
#[derive(Serialize)]
pub struct AccountField {
    pub name: String,
    pub value: String,
    verified_at: Option<DateTime<Utc>>,
    is_legacy_proof: bool,
}

/// Contains only public information
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AccountPaymentOption {
    Link { name: String, href: String },
    EthereumSubscription { chain_id: ChainId },
    MoneroSubscription {
        chain_id: ChainId,
        price: u64,
        object_id: Option<String>,
    },
}
/// https://docs.joinmastodon.org/entities/source/
#[derive(Serialize)]
pub struct Source {
    pub note: Option<String>,
    pub fields: Vec<AccountField>,
}

/// https://docs.joinmastodon.org/entities/Role/
#[derive(Serialize)]
pub struct ApiRole {
    pub id: i32,
    pub name: String,
    pub permissions: Vec<String>,
}

impl ApiRole {
    fn from_db(role: Role) -> Self {
        let role_name = match role {
            Role::Guest => unimplemented!(),
            Role::NormalUser => "user",
            Role::Admin => "admin",
            Role::ReadOnlyUser => "read_only_user",
        };
        // Mastodon 4.0 uses bitmask
        let permissions = role.get_permissions().iter()
            .map(|permission| {
                match permission {
                    Permission::CreateFollowRequest => "create_follow_request",
                    Permission::CreatePost => "create_post",
                    Permission::DeleteAnyPost => "delete_any_post",
                    Permission::DeleteAnyProfile => "delete_any_profile",
                    Permission::ManageSubscriptionOptions =>
                        "manage_subscription_options",
                }.to_string()
            })
            .collect();
        Self {
            id: i16::from(&role).into(),
            name: role_name.to_string(),
            permissions: permissions,
        }
    }
}

/// https://docs.joinmastodon.org/entities/account/
#[derive(Serialize)]
pub struct Account {
    pub id: Uuid,
    pub username: String,
    pub acct: String,
    actor_id: String, // not part of Mastodon API
    pub url: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub note: Option<String>,
    pub avatar: Option<String>,
    pub header: Option<String>,
    pub locked: bool,
    pub mention_policy: String,
    pub bot: bool,
    pub identity_proofs: Vec<AccountField>,
    pub payment_options: Vec<AccountPaymentOption>,
    pub fields: Vec<AccountField>,
    pub emojis: Vec<CustomEmoji>,
    pub followers_count: i32,
    pub following_count: i32,
    pub subscribers_count: i32,
    pub statuses_count: i32,

    // CredentialAccount attributes
    pub source: Option<Source>,
    pub role: Option<ApiRole>,
    pub authentication_methods: Option<Vec<String>>,
    pub client_config: Option<ClientConfig>,
}

impl Account {
    pub fn from_profile(
        base_url: &str,
        instance_url: &str,
        profile: DbActorProfile,
    ) -> Self {
        let actor_id = profile_actor_id(instance_url, &profile);
        let profile_url = profile_actor_url(instance_url, &profile);
        let mention_policy = match profile.mention_policy {
            MentionPolicy::None => "none",
            MentionPolicy::OnlyKnown => "only_known",
        };
        let is_automated = profile.is_automated();

        let avatar_url = profile.avatar
            .map(|image| get_file_url(base_url, &image.file_name));
        let header_url = profile.banner
            .map(|image| get_file_url(base_url, &image.file_name));

        let mut identity_proofs = vec![];
        for proof in profile.identity_proofs.into_inner() {
            let (field_name, field_value) = match proof.issuer {
                Did::Key(did_key) => {
                    ("Key".to_string(), did_key.key_multibase())
                },
                Did::Pkh(did_pkh) => {
                    let field_name = did_pkh.currency()
                        .map(|currency| currency.field_name())
                        .unwrap_or("$".to_string());
                    (field_name, did_pkh.address())
                }
            };
            let field = AccountField {
                name: field_name,
                value: field_value,
                // Use current time because DID proofs are always valid
                verified_at: Some(Utc::now()),
                is_legacy_proof: proof.proof_type.is_legacy(),
            };
            identity_proofs.push(field);
        };

        let mut extra_fields = vec![];
        for extra_field in profile.extra_fields.into_inner() {
            let field = AccountField {
                name: extra_field.name,
                value: extra_field.value,
                verified_at: None,
                is_legacy_proof: false,
            };
            extra_fields.push(field);
        };

        let payment_options = profile.payment_options.into_inner()
            .into_iter()
            .map(|option| {
                match option {
                    PaymentOption::Link(link) => {
                        AccountPaymentOption::Link {
                            name: link.name,
                            href: link.href,
                        }
                    },
                    PaymentOption::EthereumSubscription(payment_info) => {
                        AccountPaymentOption::EthereumSubscription {
                            chain_id: payment_info.chain_id,
                        }
                    },
                    PaymentOption::MoneroSubscription(payment_info) => {
                        AccountPaymentOption::MoneroSubscription {
                            chain_id: payment_info.chain_id,
                            price: payment_info.price.into(),
                            object_id: None,
                        }
                    },
                    PaymentOption::RemoteMoneroSubscription(payment_info) => {
                        AccountPaymentOption::MoneroSubscription {
                            chain_id: payment_info.chain_id,
                            price: payment_info.price.into(),
                            object_id: Some(payment_info.object_id),
                        }
                    },
                }
            })
            .collect();

        let emojis = profile.emojis.into_inner()
            .into_iter()
            .map(|db_emoji| CustomEmoji::from_db(base_url, db_emoji))
            .collect();

        Self {
            id: profile.id,
            username: profile.username,
            acct: profile.acct,
            actor_id: actor_id,
            url: profile_url,
            display_name: profile.display_name,
            created_at: profile.created_at,
            note: profile.bio,
            avatar: avatar_url,
            header: header_url,
            locked: profile.manually_approves_followers,
            mention_policy: mention_policy.to_string(),
            bot: is_automated,
            identity_proofs,
            payment_options,
            fields: extra_fields,
            emojis,
            followers_count: profile.follower_count,
            following_count: profile.following_count,
            subscribers_count: profile.subscriber_count,
            statuses_count: profile.post_count,
            source: None,
            role: None,
            authentication_methods: None,
            client_config: None,
        }
    }

    pub fn from_user(
        base_url: &str,
        instance_url: &str,
        user: User,
    ) -> Self {
        let fields_sources = user.profile.extra_fields.clone()
            .into_inner().into_iter()
            .map(|field| AccountField {
                name: field.name,
                value: field.value_source.unwrap_or(field.value),
                verified_at: None,
                is_legacy_proof: false,
            })
            .collect();
        let source = Source {
            note: user.profile.bio_source.clone(),
            fields: fields_sources,
        };
        let role = ApiRole::from_db(user.role);
        let mut authentication_methods = vec![];
        if user.password_hash.is_some() {
            authentication_methods.push(AUTHENTICATION_METHOD_PASSWORD.to_string());
        };
        if user.login_address_ethereum.is_some() {
            authentication_methods.push(AUTHENTICATION_METHOD_EIP4361.to_string());
        };
        if user.login_address_monero.is_some() {
            authentication_methods.push(AUTHENTICATION_METHOD_CAIP122_MONERO.to_string());
        };
        let mut account = Self::from_profile(
            base_url,
            instance_url,
            user.profile,
        );
        account.source = Some(source);
        account.role = Some(role);
        account.authentication_methods = Some(authentication_methods);
        account.client_config = Some(user.client_config);
        account
    }
}

fn default_authentication_method() -> String { AUTHENTICATION_METHOD_PASSWORD.to_string() }

/// https://docs.joinmastodon.org/methods/accounts/
#[derive(Deserialize)]
pub struct AccountCreateData {
    #[serde(default = "default_authentication_method")]
    pub authentication_method: String,

    pub username: String,
    pub password: Option<String>,

    pub message: Option<String>,
    pub signature: Option<String>,

    pub invite_code: Option<String>,
}

#[derive(Deserialize)]
struct AccountFieldSource {
    name: String,
    value: String,
}

#[derive(Deserialize)]
pub struct AccountUpdateData {
    display_name: Option<String>,
    note: Option<String>,
    avatar: Option<String>,
    avatar_media_type: Option<String>,
    header: Option<String>,
    header_media_type: Option<String>,
    #[serde(default)]
    locked: bool,
    fields_attributes: Option<Vec<AccountFieldSource>>,

    // Not supported by Mastodon API clients
    mention_policy: Option<String>,
}

fn process_b64_image_field_value(
    form_value: Option<String>,
    form_media_type: Option<String>,
    db_value: Option<ProfileImage>,
    storage: &MediaStorage,
) -> Result<Option<ProfileImage>, UploadError> {
    let maybe_file_name = match form_value {
        Some(b64_data) => {
            if b64_data.is_empty() {
                // Remove file
                None
            } else {
                // Decode and save file
                let media_type = form_media_type
                    .ok_or(UploadError::NoMediaType)?;
                let (file_name, file_size, media_type) = save_b64_file(
                    &b64_data,
                    &media_type,
                    storage,
                    PROFILE_IMAGE_SIZE_MAX,
                    &allowed_profile_image_media_types(&storage.supported_media_types()),
                )?;
                let image = ProfileImage::new(
                    file_name,
                    file_size,
                    media_type,
                );
                Some(image)
            }
        },
        // Keep current value
        None => db_value,
    };
    Ok(maybe_file_name)
}

impl AccountUpdateData {
    pub fn into_profile_data(
        self,
        profile: &DbActorProfile,
        storage: &MediaStorage,
    ) -> Result<ProfileUpdateData, MastodonError> {
        assert!(profile.is_local());
        let mut profile_data = ProfileUpdateData::from(profile);

        profile_data.display_name = self.display_name;
        profile_data.bio = if let Some(ref bio_source) = self.note {
            let bio = markdown_basic_to_html(bio_source)
                .map_err(|_| ValidationError("invalid markdown"))?;
            Some(bio)
        } else {
            None
        };
        profile_data.bio_source = self.note;
        profile_data.avatar = process_b64_image_field_value(
            self.avatar,
            self.avatar_media_type,
            profile.avatar.clone(),
            storage,
        )?;
        profile_data.banner = process_b64_image_field_value(
            self.header,
            self.header_media_type,
            profile.banner.clone(),
            storage,
        )?;
        profile_data.manually_approves_followers = self.locked;
        if let Some(mention_policy) = self.mention_policy {
            // Update only if value was provided by client
            profile_data.mention_policy = match mention_policy.as_str() {
                "none" => MentionPolicy::None,
                "only_known" => MentionPolicy::OnlyKnown,
                _ => return Err(ValidationError("invalid mention policy").into()),
            };
        };

        let mut extra_fields = vec![];
        for field_source in self.fields_attributes.unwrap_or(vec![]) {
            let value = markdown_basic_to_html(&field_source.value)
                .map_err(|_| ValidationError("invalid markdown"))?;
            let extra_field = ExtraField {
                name: field_source.name,
                value: value,
                value_source: Some(field_source.value),
            };
            extra_fields.push(extra_field);
        };
        profile_data.extra_fields = extra_fields;
        Ok(profile_data)
    }
}

#[derive(Serialize)]
pub struct UnsignedActivity {
    pub value: JsonValue,
}

#[derive(Deserialize)]
pub struct IdentityClaimQueryParams {
    pub proof_type: String,
    pub signer: String,
}

#[derive(Serialize)]
pub struct IdentityClaim {
    pub did: Did,
    pub claim: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct IdentityProofData {
    pub proof_type: String,
    pub did: String,
    pub signature: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct RelationshipQueryParams {
    pub id: Vec<Uuid>,
}

#[derive(Serialize)]
pub struct RelationshipMap {
    pub id: Uuid, // target ID
    pub following: bool,
    pub followed_by: bool,
    pub requested: bool,
    pub requested_by: bool,
    pub rejected_by: bool,
    pub subscription_to: bool,
    pub subscription_from: bool,
    pub showing_reblogs: bool,
    pub showing_replies: bool,
    pub muting: bool,
}

fn default_showing_reblogs() -> bool { true }

fn default_showing_replies() -> bool { true }

impl Default for RelationshipMap {
    fn default() -> Self {
        Self {
            id: Default::default(),
            following: false,
            followed_by: false,
            requested: false,
            requested_by: false,
            rejected_by: false,
            subscription_to: false,
            subscription_from: false,
            showing_reblogs: default_showing_reblogs(),
            showing_replies: default_showing_replies(),
            muting: false,
        }
    }
}

#[derive(Deserialize)]
pub struct LookupAcctQueryParams {
    pub acct: String,
}

fn default_search_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct SearchAcctQueryParams {
    pub q: String,

    #[serde(default)]
    pub resolve: bool,

    #[serde(default = "default_search_page_size")]
    pub limit: PageSize,
}

#[derive(Deserialize)]
pub struct SearchDidQueryParams {
    pub did: String,
}

#[derive(Deserialize)]
pub struct FollowData {
    #[serde(default = "default_showing_reblogs")]
    pub reblogs: bool,
    #[serde(default = "default_showing_replies")]
    pub replies: bool,
}

impl Default for FollowData {
    fn default() -> Self {
        Self {
            reblogs: default_showing_reblogs(),
            replies: default_showing_replies(),
        }
    }
}

fn default_status_page_size() -> PageSize { PageSize::new(20) }

const fn default_only_media() -> bool { false }

const fn default_exclude_replies() -> bool { true }

#[derive(Deserialize)]
pub struct StatusListQueryParams {
    #[serde(default = "default_only_media")]
    pub only_media: bool,

    #[serde(default = "default_exclude_replies")]
    pub exclude_replies: bool,

    #[serde(default)]
    pub pinned: bool,

    pub max_id: Option<Uuid>,

    #[serde(default = "default_status_page_size")]
    pub limit: PageSize,
}

fn default_follow_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct FollowListQueryParams {
    pub max_id: Option<i32>,

    #[serde(default = "default_follow_list_page_size")]
    pub limit: PageSize,
}

#[derive(Deserialize)]
pub struct SubscriptionListQueryParams {
    #[serde(default)]
    pub include_expired: bool,

    pub max_id: Option<i32>,

    #[serde(default = "default_follow_list_page_size")]
    pub limit: PageSize,
}

#[derive(Serialize)]
pub struct ApiSubscription {
    pub id: i32,
    pub sender: Account,
    pub sender_address: Option<String>,
    pub expires_at: DateTime<Utc>,
}

impl ApiSubscription {
    pub fn from_subscription(
        base_url: &str,
        instance_url: &str,
        subscription: Subscription,
    ) -> Self {
        let sender = Account::from_profile(
            base_url,
            instance_url,
            subscription.sender,
        );
        Self {
            id: subscription.id,
            sender,
            sender_address: subscription.sender_address,
            expires_at: subscription.expires_at,
        }
    }
}

#[derive(Serialize)]
pub struct Alias {
    pub id: String,
    pub account: Option<Account>,
}

#[derive(Serialize)]
pub struct Aliases {
    pub declared: Vec<Account>,
    pub declared_all: Vec<Alias>,
    pub verified: Vec<Account>,
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::ProfileImage;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_create_account_from_profile() {
        let profile = DbActorProfile {
            avatar: Some(ProfileImage::new(
                "test".to_string(),
                1000,
                "image/png".to_string(),
            )),
            ..Default::default()
        };
        let account = Account::from_profile(
            INSTANCE_URL,
            INSTANCE_URL,
            profile,
        );

        assert_eq!(
            account.avatar.unwrap(),
            format!("{}/media/test", INSTANCE_URL),
        );
        assert!(account.source.is_none());
    }

    #[test]
    fn test_create_account_from_user() {
        let bio_source = "test";
        let login_address = "0x1234";
        let profile = DbActorProfile {
            bio_source: Some(bio_source.to_string()),
            ..Default::default()
        };
        let user = User {
            login_address_ethereum: Some(login_address.to_string()),
            profile,
            ..Default::default()
        };
        let account = Account::from_user(
            INSTANCE_URL,
            INSTANCE_URL,
            user,
        );

        assert_eq!(
            account.source.unwrap().note.unwrap(),
            bio_source,
        );
    }
}
