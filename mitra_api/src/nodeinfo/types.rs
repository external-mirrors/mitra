// http://nodeinfo.diaspora.software/schema.html
use serde::Serialize;

use mitra_config::{
    Config,
    RegistrationType,
    SoftwareMetadata,
};

const ATOM_SERVICE: &str = "atom1.0";
const ACTIVITYPUB_PROTOCOL: &str = "activitypub";

#[derive(Serialize)]
struct Software20 {
    name: String,
    version: String,
}

impl From<SoftwareMetadata> for Software20 {
    fn from(software: SoftwareMetadata) -> Self {
        Self {
            name: software.name.to_lowercase(),
            version: software.version.to_owned(),
        }
    }
}

#[derive(Serialize)]
struct Software21 {
    name: String,
    version: String,
    repository: String,
}

impl From<SoftwareMetadata> for Software21 {
    fn from(software: SoftwareMetadata) -> Self {
        Self {
            name: software.name.to_lowercase(),
            version: software.version.to_owned(),
            repository: software.repository.to_owned(),
        }
    }
}

#[derive(Serialize)]
struct Services {
    inbound: Vec<&'static str>,
    outbound: Vec<&'static str>,
}

impl Default for Services {
    fn default() -> Self {
        Self {
            inbound: vec![],
            outbound: vec![ATOM_SERVICE],
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Users {
    pub total: i64,
    pub active_halfyear: i64,
    pub active_month: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub users: Users,
    pub local_posts: i64,
}

#[derive(Serialize)]
struct FederationMetadata {
    enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    node_name: String,
    node_description: String,

    // Pleroma extensions
    federation: FederationMetadata,
    staff_accounts: Vec<String>,
}

impl Metadata {
    pub fn new(config: &Config, instance_staff: Vec<String>) -> Self {
        Self {
            node_name: config.instance_title.clone(),
            node_description: config.instance_short_description.clone(),
            federation: FederationMetadata {
                enabled: config.federation.enabled,
            },
            staff_accounts: instance_staff,
        }
    }
}

fn has_open_registrations(config: &Config) -> bool {
    config.registration.registration_type != RegistrationType::Invite
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo20 {
    version: &'static str,
    software: Software20,
    protocols: Vec<&'static str>,
    services: Services,
    open_registrations: bool,
    usage: Usage,
    metadata: Metadata,
}

impl NodeInfo20 {
    pub fn new(config: &Config, usage: Usage, metadata: Metadata) -> Self {
        Self {
            version: "2.0",
            software: Software20::from(config.software),
            protocols: vec![ACTIVITYPUB_PROTOCOL],
            services: Services::default(),
            open_registrations: has_open_registrations(config),
            usage,
            metadata,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo21 {
    version: &'static str,
    software: Software21,
    protocols: Vec<&'static str>,
    services: Services,
    open_registrations: bool,
    usage: Usage,
    metadata: Metadata,
}

impl NodeInfo21 {
    pub fn new(config: &Config, usage: Usage, metadata: Metadata) -> Self {
        Self {
            version: "2.1",
            software: Software21::from(config.software),
            protocols: vec![ACTIVITYPUB_PROTOCOL],
            services: Services::default(),
            open_registrations: has_open_registrations(config),
            usage,
            metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_software() {
        let software_metadata = SoftwareMetadata {
            name: "Mastodon",
            version: "4.5.0",
            repository: "https://example.com",
        };
        let software_2_0 = Software20::from(software_metadata);
        assert_eq!(software_2_0.name, "mastodon");
        let software_2_1 = Software20::from(software_metadata);
        assert_eq!(software_2_1.name, "mastodon");
    }
}
