use serde::Deserialize;

use mitra_models::profiles::queries::ProfileOrder;

use crate::mastodon_api::{
    pagination::PageSize,
    serializers::deserialize_boolean,
};

const DIRECTORY_ORDER_ACTIVE: &str = "active";

fn default_page_size() -> PageSize { PageSize::new(40) }

fn default_order() -> String { DIRECTORY_ORDER_ACTIVE.to_owned() }

fn default_only_local() -> bool { true }

/// https://docs.joinmastodon.org/methods/instance/directory/
#[derive(Deserialize)]
pub struct DirectoryQueryParams {
    #[serde(default)]
    pub offset: u16,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,

    #[serde(default = "default_order")]
    order: String,

    #[serde(
        default = "default_only_local",
        deserialize_with = "deserialize_boolean",
    )]
    pub local: bool,
}

impl DirectoryQueryParams {
    pub fn db_order(&self) -> ProfileOrder {
        if self.order == DIRECTORY_ORDER_ACTIVE {
            ProfileOrder::Active
        } else {
            ProfileOrder::Username
        }
    }
}
