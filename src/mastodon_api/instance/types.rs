use serde::Serialize;
use serde_json::{json, Value};

use mitra_adapters::dynamic_config::DynamicConfig;
use mitra_config::{
    AuthenticationMethod,
    BlockchainConfig,
    Config,
    RegistrationType,
    SOFTWARE_NAME,
    SOFTWARE_REPOSITORY,
    SOFTWARE_VERSION,
};
use mitra_models::users::types::User;
use mitra_services::{
    media::MediaStorage,
};
use mitra_utils::markdown::markdown_to_html;
use mitra_validators::posts::ATTACHMENT_LIMIT;

use crate::mastodon_api::{
    accounts::types::{
        Account,
        AUTHENTICATION_METHOD_CAIP122_MONERO,
        AUTHENTICATION_METHOD_PASSWORD,
        AUTHENTICATION_METHOD_EIP4361,
    },
    MASTODON_API_VERSION,
};

#[derive(Serialize)]
struct Stats {
    user_count: i64,
    status_count: i64,
    domain_count: i64,
}

#[derive(Serialize)]
struct StatusLimits {
    max_characters: usize,
    max_media_attachments: usize,
}

#[derive(Serialize)]
struct MediaLimits {
    supported_mime_types: Vec<String>,
    image_size_limit: usize,
}

#[derive(Serialize)]
struct Configuration {
    statuses: StatusLimits,
    media_attachments: MediaLimits,
}

#[derive(Serialize)]
struct AllowUnauthenticated {
    timeline_local: bool,
}

#[derive(Serialize)]
struct BlockchainFeatures {
    subscriptions: bool,
}

#[derive(Serialize)]
struct BlockchainInfo {
    chain_id: String,
    chain_metadata: Option<Value>,
    features: BlockchainFeatures,
}

#[derive(Serialize)]
struct PleromaMetadata {
    features: [&'static str; 3],
}

#[derive(Serialize)]
struct PleromaInfo {
    metadata: PleromaMetadata,
}

/// https://docs.joinmastodon.org/entities/V1_Instance/
#[derive(Serialize)]
pub struct InstanceInfo {
    uri: String,
    title: String,
    short_description: String,
    description: String,
    version: String,
    registrations: bool,
    approval_required: bool,
    invites_enabled: bool,
    stats: Stats,
    configuration: Configuration,
    contact_account: Option<Account>,

    authentication_methods: Vec<String>,
    login_message: String,
    // Similar to pleroma.restrict_unauthenticated
    allow_unauthenticated: AllowUnauthenticated,
    federated_timeline_restricted: bool, // from dynamic config

    blockchains: Vec<BlockchainInfo>,
    ipfs_gateway_url: Option<String>,

    pleroma: PleromaInfo,
}

fn get_full_api_version(version: &str) -> String {
    format!(
        "{api_version} (compatible; {name} {version})",
        api_version=MASTODON_API_VERSION,
        name=SOFTWARE_NAME,
        version=version,
    )
}

impl InstanceInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        base_url: &str,
        config: &Config,
        dynamic_config: DynamicConfig,
        maybe_admin: Option<User>,
        user_count: i64,
        post_count: i64,
        peer_count: i64,
    ) -> Self {
        let blockchains = config.blockchains().iter().filter_map(|item| match item {
            BlockchainConfig::Ethereum(_) => None,
            BlockchainConfig::Monero(monero_config) => {
                let features = BlockchainFeatures {
                    subscriptions: true,
                };
                let maybe_chain_metadata = monero_config
                    .chain_metadata.as_ref()
                    .and_then(|metadata| metadata.description.as_ref())
                    .map(|text| markdown_to_html(text))
                    .map(|html| json!({"description": html}));
                Some(BlockchainInfo {
                    chain_id: monero_config.chain_id.to_string(),
                    chain_metadata: maybe_chain_metadata,
                    features: features,
                })
            },
        }).collect();
        Self {
            uri: config.instance().hostname(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: markdown_to_html(&config.instance_description),
            version: get_full_api_version(SOFTWARE_VERSION),
            registrations:
                config.registration.registration_type !=
                RegistrationType::Invite,
            approval_required: false,
            invites_enabled:
                config.registration.registration_type ==
                RegistrationType::Invite,
            stats: Stats {
                user_count,
                status_count: post_count,
                domain_count: peer_count,
            },
            configuration: Configuration {
                statuses: StatusLimits {
                    max_characters: config.limits.posts.character_limit,
                    max_media_attachments: ATTACHMENT_LIMIT,
                },
                media_attachments: MediaLimits {
                    supported_mime_types: MediaStorage::from(config)
                        .supported_media_types().iter()
                        .map(|media_type| media_type.to_string()).collect(),
                    image_size_limit: config.limits.media.file_size_limit,
                },
            },
            contact_account: maybe_admin.map(|user| Account::from_profile(
                base_url,
                &config.instance().url(),
                user.profile,
            )),
            authentication_methods: config.authentication_methods.iter()
                .map(|method| {
                    let value = match method {
                        AuthenticationMethod::Password => AUTHENTICATION_METHOD_PASSWORD,
                        AuthenticationMethod::Eip4361 => AUTHENTICATION_METHOD_EIP4361,
                        AuthenticationMethod::Caip122Monero => AUTHENTICATION_METHOD_CAIP122_MONERO,
                    };
                    value.to_string()
                })
                .collect(),
            login_message: config.login_message.clone(),
            allow_unauthenticated: AllowUnauthenticated {
                timeline_local: config.instance_timeline_public,
            },
            federated_timeline_restricted: dynamic_config.federated_timeline_restricted,
            blockchains: blockchains,
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
            pleroma: PleromaInfo {
                metadata: PleromaMetadata {
                    features: [
                        "quote_posting",
                        "pleroma_emoji_reactions",
                        "pleroma_custom_emoji_reactions",
                    ],
                },
            },
        }
    }
}

#[derive(Serialize)]
struct UsageUsers {
    active_month: i64,
}

#[derive(Serialize)]
struct Usage {
    users: UsageUsers,
}

#[derive(Serialize)]
struct ConfigurationV2 {
    statuses: StatusLimits,
    media_attachments: MediaLimits,
}

#[derive(Serialize)]
struct Registrations {
    enabled: bool,
    approval_required: bool,
    message: Option<String>,
}

#[derive(Serialize)]
struct Contact {
    email: String,
    account: Option<Account>,
}

/// https://docs.joinmastodon.org/entities/Instance/
#[derive(Serialize)]
pub struct InstanceInfoV2 {
    domain: String,
    title: String,
    description: String,
    version: String,
    source_url: String,
    usage: Usage,
    configuration: ConfigurationV2,
    registrations: Registrations,
    contact: Contact,
}

impl InstanceInfoV2 {
    pub fn create(
        base_url: &str,
        config: &Config,
        maybe_admin: Option<User>,
        user_count_active_month: i64,
    ) -> Self {
        Self {
            domain: config.instance().hostname(),
            title: config.instance_title.clone(),
            description: config.instance_short_description.clone(),
            version: get_full_api_version(SOFTWARE_VERSION),
            source_url: SOFTWARE_REPOSITORY.to_string(),
            usage: Usage {
                users: UsageUsers {
                    active_month: user_count_active_month,
                },
            },
            configuration: ConfigurationV2 {
                statuses: StatusLimits {
                    max_characters: config.limits.posts.character_limit,
                    max_media_attachments: ATTACHMENT_LIMIT,
                },
                media_attachments: MediaLimits {
                    supported_mime_types: MediaStorage::from(config)
                        .supported_media_types().iter()
                        .map(|media_type| media_type.to_string()).collect(),
                    image_size_limit: config.limits.media.file_size_limit,
                },
            },
            registrations: Registrations {
                enabled:
                    config.registration.registration_type !=
                    RegistrationType::Invite,
                approval_required: false,
                message: None,
            },
            contact: Contact {
                email: "".to_string(),
                account: maybe_admin.map(|user| Account::from_profile(
                    base_url,
                    &config.instance().url(),
                    user.profile,
                )),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_full_api_version() {
        let full_version = get_full_api_version("2.0.0");
        assert_eq!(full_version, "4.0.0 (compatible; Mitra 2.0.0)");
    }
}
