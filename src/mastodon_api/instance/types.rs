use serde::Serialize;
use serde_json::{json, to_value, Value};

use mitra_config::{
    AuthenticationMethod,
    BlockchainConfig,
    Config,
    RegistrationType,
    MITRA_VERSION,
};
use mitra_models::users::types::User;
use mitra_services::ethereum::contracts::ContractSet;
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
use crate::media::MediaStorage;

#[derive(Serialize)]
struct InstanceStats {
    user_count: i64,
    status_count: i64,
    domain_count: i64,
}

#[derive(Serialize)]
struct InstanceStatusLimits {
    max_characters: usize,
    max_media_attachments: usize,
}

#[derive(Serialize)]
struct InstanceMediaLimits {
    supported_mime_types: Vec<String>,
    image_size_limit: usize,
}

#[derive(Serialize)]
struct InstanceConfiguration {
    statuses: InstanceStatusLimits,
    media_attachments: InstanceMediaLimits,
}

#[derive(Serialize)]
struct AllowUnauthenticated {
    timeline_local: bool,
}

#[derive(Serialize)]
struct BlockchainFeatures {
    gate: bool,
    minter: bool,
    subscriptions: bool,
}

#[derive(Serialize)]
struct BlockchainInfo {
    chain_id: String,
    chain_metadata: Option<Value>,
    contract_address: Option<String>,
    features: BlockchainFeatures,
}

/// https://docs.joinmastodon.org/entities/V1_Instance/
#[derive(Serialize)]
pub struct InstanceInfo {
    uri: String,
    title: String,
    short_description: String,
    description: String,
    description_source: String,
    version: String,
    registrations: bool,
    approval_required: bool,
    invites_enabled: bool,
    stats: InstanceStats,
    configuration: InstanceConfiguration,
    contact_account: Option<Account>,

    authentication_methods: Vec<String>,
    login_message: String,
    // Similar to pleroma.restrict_unauthenticated
    allow_unauthenticated: AllowUnauthenticated,
    blockchains: Vec<BlockchainInfo>,
    ipfs_gateway_url: Option<String>,
}

fn get_full_api_version(version: &str) -> String {
    format!(
        "{0} (compatible; Mitra {1})",
        MASTODON_API_VERSION,
        version,
    )
}

impl InstanceInfo {
    pub fn create(
        base_url: &str,
        config: &Config,
        maybe_admin: Option<User>,
        maybe_ethereum_contracts: Option<&ContractSet>,
        user_count: i64,
        post_count: i64,
        peer_count: i64,
    ) -> Self {
        let blockchains = config.blockchains().iter().map(|item| match item {
            BlockchainConfig::Ethereum(ethereum_config) => {
                let features = if let Some(contract_set) = maybe_ethereum_contracts {
                    BlockchainFeatures {
                        gate: contract_set.gate.is_some(),
                        minter: contract_set.collectible.is_some(),
                        subscriptions: contract_set.subscription.is_some(),
                    }
                } else {
                    BlockchainFeatures {
                        gate: false,
                        minter: false,
                        subscriptions: false,
                    }
                };
                let maybe_chain_metadata = ethereum_config
                    .chain_metadata.as_ref()
                    .and_then(|metadata| to_value(metadata).ok());
                BlockchainInfo {
                    chain_id: ethereum_config.chain_id.to_string(),
                    chain_metadata: maybe_chain_metadata,
                    contract_address:
                        Some(ethereum_config.contract_address.clone()),
                    features: features,
                }
            },
            BlockchainConfig::Monero(monero_config) => {
                let features = BlockchainFeatures {
                    gate: false,
                    minter: false,
                    subscriptions: true,
                };
                let maybe_chain_metadata = monero_config
                    .chain_metadata.as_ref()
                    .and_then(|metadata| metadata.description.as_ref())
                    .map(|text| markdown_to_html(text))
                    .map(|html| json!({"description": html}));
                BlockchainInfo {
                    chain_id: monero_config.chain_id.to_string(),
                    chain_metadata: maybe_chain_metadata,
                    contract_address: None,
                    features: features,
                }
            },
        }).collect();
        Self {
            uri: config.instance().hostname(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: markdown_to_html(&config.instance_description),
            description_source: config.instance_description.clone(),
            version: get_full_api_version(MITRA_VERSION),
            registrations:
                config.registration.registration_type !=
                RegistrationType::Invite,
            approval_required: false,
            invites_enabled:
                config.registration.registration_type ==
                RegistrationType::Invite,
            stats: InstanceStats {
                user_count,
                status_count: post_count,
                domain_count: peer_count,
            },
            configuration: InstanceConfiguration {
                statuses: InstanceStatusLimits {
                    max_characters: config.limits.posts.character_limit,
                    max_media_attachments: ATTACHMENT_LIMIT,
                },
                media_attachments: InstanceMediaLimits {
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
            blockchains: blockchains,
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
