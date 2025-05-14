use std::ops::Not;

use actix_multipart::form::{
    bytes::Bytes,
    text::Text,
    MultipartForm,
};
use apx_core::{
    base64,
    caip2::ChainId,
    did::Did,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_activitypub::identifiers::{
    profile_actor_id,
    profile_actor_url,
};
use mitra_adapters::payments::subscriptions::MONERO_PAYMENT_AMOUNT_MIN;
use mitra_config::MediaLimits;
use mitra_models::{
    media::types::{MediaInfo, PartialMediaInfo},
    posts::types::Visibility,
    profiles::types::{
        DbActorProfile,
        ExtraField,
        MentionPolicy,
        PaymentOption,
        ProfileUpdateData,
    },
    subscriptions::types::{SubscriptionDetailed as DbSubscriptionDetailed},
    users::types::{
        ClientConfig,
        Permission,
        Role as DbRole,
        SharedClientConfig,
        User,
    },
};
use mitra_services::media::MediaStorage;
use mitra_utils::{
    currencies::Currency,
    markdown::markdown_basic_to_html,
};
use mitra_validators::{
    errors::ValidationError,
    profiles::{
        allowed_profile_image_media_types,
        clean_extra_field,
    },
};

use crate::mastodon_api::{
    custom_emojis::types::CustomEmoji,
    errors::MastodonError,
    media_server::ClientMediaServer,
    pagination::PageSize,
    serializers::{
        deserialize_boolean,
        serialize_datetime,
        serialize_datetime_opt,
    },
    statuses::types::{visibility_from_str, visibility_to_str},
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
    #[serde(serialize_with = "serialize_datetime_opt")]
    verified_at: Option<DateTime<Utc>>,
    is_legacy_proof: bool,
}

/// Contains only public information
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AccountPaymentOption {
    Link { name: String, href: String },
    MoneroSubscription {
        chain_id: ChainId,
        price: u64,
        amount_min: u64,
        object_id: Option<String>,
    },
}

// https://docs.joinmastodon.org/entities/Account/#source
#[derive(Serialize)]
pub struct AccountSource {
    pub note: Option<String>,
    pub fields: Vec<AccountField>,
    privacy: &'static str,
    sensitive: bool,
}

// https://docs.joinmastodon.org/entities/Role/
#[derive(Serialize)]
pub struct Role {
    pub id: i32,
    pub name: String,
    pub permissions: String,
    pub permissions_names: Vec<String>,
}

impl Role {
    fn from_db(role: DbRole) -> Self {
        let role_name = match role {
            DbRole::Guest => unimplemented!(),
            DbRole::NormalUser => "user",
            DbRole::Admin => "admin",
            DbRole::ReadOnlyUser => "read_only_user",
        };
        let mut permissions = vec![];
        // Mastodon uses bitmask
        // https://docs.joinmastodon.org/entities/Role/#permissions
        let mut bitmask = 0;
        for permission in role.get_permissions() {
            let (name, bit) = match permission {
                Permission::CreateFollowRequest =>
                    ("create_follow_request", 0x0),
                Permission::CreatePost =>
                    ("create_post", 0x0),
                Permission::DeleteAnyPost =>
                    ("delete_any_post", 0x1),
                Permission::DeleteAnyProfile =>
                    ("delete_any_profile", 0x1),
                Permission::ManageSubscriptionOptions =>
                    ("manage_subscription_options", 0x0),
            };
            permissions.push(name.to_owned());
            if bitmask & bit == 0 {
                bitmask += bit;
            };
        };
        Self {
            id: i16::from(role).into(),
            name: role_name.to_string(),
            permissions: bitmask.to_string(),
            permissions_names: permissions,
        }
    }
}

fn mention_policy_to_str(mention_policy: MentionPolicy) -> &'static str {
    match mention_policy {
        MentionPolicy::None => "none",
        MentionPolicy::OnlyKnown => "only_known",
        MentionPolicy::OnlyContacts => "only_contacts",
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
    #[serde(serialize_with = "serialize_datetime")]
    pub created_at: DateTime<Utc>,
    pub note: String,
    pub avatar: String,
    pub header: String,
    pub locked: bool,
    pub mention_policy: String,
    pub bot: bool,
    discoverable: bool,
    pub identity_proofs: Vec<AccountField>,
    pub payment_options: Vec<AccountPaymentOption>,
    pub fields: Vec<AccountField>,
    pub emojis: Vec<CustomEmoji>,
    pub followers_count: i32,
    pub following_count: i32,
    pub subscribers_count: i32,
    pub statuses_count: i32,

    // CredentialAccount attributes
    pub source: Option<AccountSource>,
    pub role: Option<Role>,
    pub authentication_methods: Option<Vec<String>>,
    pub client_config: Option<ClientConfig>,
}

impl Account {
    pub fn from_profile(
        instance_uri: &str,
        media_server: &ClientMediaServer,
        profile: DbActorProfile,
    ) -> Self {
        let actor_id = profile_actor_id(instance_uri, &profile);
        let profile_url = profile_actor_url(instance_uri, &profile);
        let preferred_handle = profile.preferred_handle().to_owned();
        let mention_policy = mention_policy_to_str(profile.mention_policy);

        let avatar_url = profile.avatar
            .map(|image| media_server.url_for(&image))
            .unwrap_or(format!(
                "{}/api/v1/accounts/identicon?input={actor_id}",
                media_server.base_url(),
            ));
        let header_url = profile.banner
            .map(|image| media_server.url_for(&image))
            .unwrap_or(format!(
                "{}/api/v1/accounts/identicon",
                media_server.base_url(),
            ));

        let mut identity_proofs = vec![];
        for proof in profile.identity_proofs.into_inner() {
            let (field_name, field_value) = match proof.issuer {
                Did::Key(did_key) => {
                    ("Key".to_string(), did_key.key_multibase())
                },
                Did::Pkh(did_pkh) => {
                    let field_name =
                        Currency::from(did_pkh.chain_id()).field_name();
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
                    PaymentOption::MoneroSubscription(payment_info) => {
                        AccountPaymentOption::MoneroSubscription {
                            chain_id: payment_info.chain_id,
                            price: payment_info.price.into(),
                            object_id: None,
                            amount_min: MONERO_PAYMENT_AMOUNT_MIN,
                        }
                    },
                    PaymentOption::RemoteMoneroSubscription(payment_info) => {
                        AccountPaymentOption::MoneroSubscription {
                            chain_id: payment_info.chain_id,
                            price: payment_info.price.into(),
                            amount_min: payment_info.amount_min
                                .unwrap_or(MONERO_PAYMENT_AMOUNT_MIN),
                            object_id: Some(payment_info.object_id),
                        }
                    },
                }
            })
            .collect();

        let emojis = profile.emojis.into_inner()
            .into_iter()
            .map(|db_emoji| CustomEmoji::from_db(media_server, db_emoji))
            .collect();

        Self {
            id: profile.id,
            username: profile.username,
            acct: preferred_handle,
            actor_id: actor_id,
            url: profile_url,
            display_name: profile.display_name,
            created_at: profile.created_at,
            note: profile.bio.unwrap_or_default(),
            avatar: avatar_url,
            header: header_url,
            locked: profile.manually_approves_followers,
            mention_policy: mention_policy.to_string(),
            bot: profile.is_automated,
            discoverable: true,
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
        instance_uri: &str,
        media_server: &ClientMediaServer,
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
        let source = AccountSource {
            note: user.profile.bio_source.clone(),
            fields: fields_sources,
            privacy: visibility_to_str(user.shared_client_config.default_post_visibility),
            sensitive: false,
        };
        let role = Role::from_db(user.role);
        let mut authentication_methods = vec![];
        if user.password_digest.is_some() {
            authentication_methods.push(AUTHENTICATION_METHOD_PASSWORD.to_string());
        };
        if user.login_address_ethereum.is_some() {
            authentication_methods.push(AUTHENTICATION_METHOD_EIP4361.to_string());
        };
        if user.login_address_monero.is_some() {
            authentication_methods.push(AUTHENTICATION_METHOD_CAIP122_MONERO.to_string());
        };
        let mut account = Self::from_profile(
            instance_uri,
            media_server,
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

// Supports partial updates
#[derive(Deserialize)]
pub struct AccountSourceData {
    privacy: Option<String>,
}

impl AccountSourceData {
    pub fn update_shared_client_config(
        &self,
        client_config: &SharedClientConfig,
    ) -> Result<SharedClientConfig, ValidationError> {
        let mut client_config = client_config.clone();
        if let Some(ref privacy) = self.privacy {
            let visibility = visibility_from_str(privacy)?;
            if !matches!(
                visibility,
                Visibility::Public | Visibility::Followers | Visibility::Subscribers,
            ) {
                return Err(ValidationError("invalid default visibility"));
            };
            client_config.default_post_visibility = visibility;
        };
        Ok(client_config)
    }
}

// Supports partial updates
#[derive(Deserialize)]
pub struct AccountUpdateData {
    display_name: Option<String>,
    note: Option<String>,
    avatar: Option<String>,
    avatar_media_type: Option<String>,
    header: Option<String>,
    header_media_type: Option<String>,
    bot: Option<bool>,
    locked: Option<bool>,
    fields_attributes: Option<Vec<AccountFieldSource>>,
    pub source: Option<AccountSourceData>,

    // Not supported by Mastodon API clients
    mention_policy: Option<String>,
}

fn process_b64_image_field_value(
    form_value: Option<String>,
    form_media_type: Option<String>,
    db_value: Option<PartialMediaInfo>,
    media_limits: &MediaLimits,
    media_storage: &MediaStorage,
) -> Result<Option<PartialMediaInfo>, UploadError> {
    let maybe_file_name = match form_value {
        Some(b64_data) => {
            if b64_data.is_empty() {
                // Remove file
                None
            } else {
                // Decode and save file
                let media_type = form_media_type
                    .ok_or(UploadError::NoMediaType)?;
                let file_info = save_b64_file(
                    &b64_data,
                    &media_type,
                    media_storage,
                    media_limits.profile_image_local_size_limit,
                    &allowed_profile_image_media_types(&media_limits.supported_media_types()),
                )?;
                let image = PartialMediaInfo::from(MediaInfo::local(file_info));
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
        media_limits: &MediaLimits,
        media_storage: &MediaStorage,
    ) -> Result<ProfileUpdateData, MastodonError> {
        assert!(profile.is_local());
        let mut profile_data = ProfileUpdateData::from(profile);
        if let Some(display_name) = self.display_name {
            profile_data.display_name = Some(display_name);
        };
        if let Some(bio_source) = self.note {
            let bio = markdown_basic_to_html(&bio_source)
                .map_err(|_| ValidationError("invalid markdown"))?;
            profile_data.bio = Some(bio);
            profile_data.bio_source = Some(bio_source);
        };
        profile_data.avatar = process_b64_image_field_value(
            self.avatar,
            self.avatar_media_type,
            profile.avatar.clone(),
            media_limits,
            media_storage,
        )?;
        profile_data.banner = process_b64_image_field_value(
            self.header,
            self.header_media_type,
            profile.banner.clone(),
            media_limits,
            media_storage,
        )?;
        if let Some(bot) = self.bot {
            profile_data.is_automated = bot;
        };
        if let Some(locked) = self.locked {
            profile_data.manually_approves_followers = locked;
        };
        if let Some(mention_policy) = self.mention_policy {
            profile_data.mention_policy = match mention_policy.as_str() {
                "none" => MentionPolicy::None,
                "only_known" => MentionPolicy::OnlyKnown,
                "only_contacts" => MentionPolicy::OnlyContacts,
                _ => return Err(ValidationError("invalid mention policy").into()),
            };
        };

        if let Some(fields_attributes) = self.fields_attributes {
            let mut extra_fields = vec![];
            for field_source in fields_attributes {
                if field_source.name.trim().is_empty() {
                    continue;
                };
                let value = markdown_basic_to_html(&field_source.value)
                    .map_err(|_| ValidationError("invalid markdown"))?;
                let mut extra_field = ExtraField {
                    name: field_source.name,
                    value: value,
                    value_source: Some(field_source.value),
                };
                clean_extra_field(&mut extra_field);
                extra_fields.push(extra_field);
            };
            profile_data.extra_fields = extra_fields;
        };
        Ok(profile_data)
    }
}

#[derive(MultipartForm)]
pub struct AccountUpdateMultipartForm {
    display_name: Option<Text<String>>,
    note: Option<Text<String>>,
    avatar: Option<Bytes>,
    header: Option<Bytes>,
    bot: Option<Text<bool>>,
    locked: Option<Text<bool>>,

    // 4 fields max
    #[multipart(rename = "fields_attributes[0][name]")]
    fields_attributes_0_name: Option<Text<String>>,
    #[multipart(rename = "fields_attributes[0][value]")]
    fields_attributes_0_value: Option<Text<String>>,
    #[multipart(rename = "fields_attributes[1][name]")]
    fields_attributes_1_name: Option<Text<String>>,
    #[multipart(rename = "fields_attributes[1][value]")]
    fields_attributes_1_value: Option<Text<String>>,
    #[multipart(rename = "fields_attributes[2][name]")]
    fields_attributes_2_name: Option<Text<String>>,
    #[multipart(rename = "fields_attributes[2][value]")]
    fields_attributes_2_value: Option<Text<String>>,
    #[multipart(rename = "fields_attributes[3][name]")]
    fields_attributes_3_name: Option<Text<String>>,
    #[multipart(rename = "fields_attributes[3][value]")]
    fields_attributes_3_value: Option<Text<String>>,

    #[multipart(rename = "source[privacy]")]
    source_privacy: Option<Text<String>>,
}

impl From<AccountUpdateMultipartForm> for AccountUpdateData {
    fn from(form: AccountUpdateMultipartForm) -> Self {
        let fields_attributes: Vec<_> = [
            (form.fields_attributes_0_name, form.fields_attributes_0_value),
            (form.fields_attributes_1_name, form.fields_attributes_1_value),
            (form.fields_attributes_2_name, form.fields_attributes_2_value),
            (form.fields_attributes_3_name, form.fields_attributes_3_value),
        ]
            .into_iter()
            .filter_map(|(maybe_name, maybe_value)| {
                match (maybe_name, maybe_value) {
                    (Some(name), Some(value)) => {
                        let field_source = AccountFieldSource {
                            name: name.into_inner(),
                            value: value.into_inner(),
                        };
                        Some(field_source)
                    },
                    _ => None,
                }
            })
            .collect();
        let source_data = AccountSourceData {
            privacy: form.source_privacy.map(|value| value.into_inner()),
        };
        Self {
            display_name: form.display_name
                .map(|value| value.into_inner()),
            note: form.note
                .map(|value| value.into_inner()),
            avatar: form.avatar.as_ref()
                .map(|file| base64::encode(&file.data)),
            avatar_media_type: form.avatar.and_then(|file| {
                file.content_type
                    .map(|media_type| media_type.essence_str().to_string())
            }),
            header: form.header.as_ref()
                .map(|file| base64::encode(&file.data)),
            header_media_type: form.header.and_then(|file| {
                file.content_type
                    .map(|media_type| media_type.essence_str().to_string())
            }),
            bot: form.bot
                .map(|value| value.into_inner()),
            locked: form.locked
                .map(|value| value.into_inner()),
            fields_attributes: fields_attributes
                .is_empty()
                .not()
                .then_some(fields_attributes),
            source: Some(source_data),
            mention_policy: None,
        }
    }
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
pub struct IdentityProofDeletionRequest {
    pub did: Did,
}

#[derive(Deserialize)]
pub struct RelationshipQueryParams {
    pub id: Vec<Uuid>,
}

// https://docs.joinmastodon.org/entities/Relationship/
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
    pub muting_notifications: bool,
    pub blocking: bool,
    pub blocked_by: bool,
    pub domain_blocking: bool,
    pub notifying: bool,
    pub endorsed: bool,
    pub languages: Vec<String>,
    pub note: String,
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
            muting_notifications: false,
            blocking: false,
            blocked_by: false,
            domain_blocking: false,
            notifying: false,
            endorsed: false,
            languages: vec![],
            note: "".to_owned(),
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

    #[serde(default, deserialize_with = "deserialize_boolean")]
    pub resolve: bool,

    #[serde(default = "default_search_page_size")]
    pub limit: PageSize,

    #[serde(default)]
    pub offset: u16,
}

#[derive(Deserialize)]
pub struct SearchDidQueryParams {
    pub did: String,
}

#[derive(Deserialize)]
pub struct IdenticonQueryParams {
    pub input: Option<String>,
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

const fn default_exclude_reblogs() -> bool { false }

#[derive(Deserialize)]
pub struct StatusListQueryParams {
    #[serde(
        default = "default_only_media",
        deserialize_with = "deserialize_boolean",
    )]
    pub only_media: bool,

    #[serde(
        default = "default_exclude_replies",
        deserialize_with = "deserialize_boolean",
    )]
    pub exclude_replies: bool,

    #[serde(
        default = "default_exclude_reblogs",
        deserialize_with = "deserialize_boolean",
    )]
    pub exclude_reblogs: bool,

    #[serde(
        default,
        deserialize_with = "deserialize_boolean",
    )]
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
pub struct Subscription {
    pub id: i32,
    pub sender: Account,
    #[serde(serialize_with = "serialize_datetime")]
    pub expires_at: DateTime<Utc>,
}

impl Subscription {
    pub fn from_db(
        instance_uri: &str,
        media_server: &ClientMediaServer,
        subscription: DbSubscriptionDetailed,
    ) -> Self {
        let sender = Account::from_profile(
            instance_uri,
            media_server,
            subscription.sender,
        );
        Self {
            id: subscription.id,
            sender,
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

fn default_actor_collection() -> String {
    "outbox".to_owned()
}

#[derive(Deserialize)]
pub struct LoadActivitiesParams {
    #[serde(default = "default_actor_collection")]
    pub collection: String,
}

#[cfg(test)]
mod tests {
    use mitra_models::{
        media::types::{MediaInfo, PartialMediaInfo},
    };
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_create_account_from_profile() {
        let media_server = ClientMediaServer::for_test(INSTANCE_URI);
        let mut profile = DbActorProfile::local_for_test("test");
        profile.avatar = Some(PartialMediaInfo::from(MediaInfo::png_for_test()));
        let account = Account::from_profile(
            INSTANCE_URI,
            &media_server,
            profile,
        );

        assert_eq!(
            account.avatar,
            format!("{}/media/test.png", INSTANCE_URI),
        );
        assert!(account.source.is_none());
    }

    #[test]
    fn test_create_account_from_user() {
        let media_server = ClientMediaServer::for_test(INSTANCE_URI);
        let bio_source = "test";
        let login_address = "0x1234";
        let mut profile = DbActorProfile::local_for_test("test");
        profile.bio_source = Some(bio_source.to_string());
        let user = User {
            login_address_ethereum: Some(login_address.to_string()),
            profile,
            ..Default::default()
        };
        let account = Account::from_user(
            INSTANCE_URI,
            &media_server,
            user,
        );

        assert_eq!(
            account.source.unwrap().note.unwrap(),
            bio_source,
        );
    }
}
