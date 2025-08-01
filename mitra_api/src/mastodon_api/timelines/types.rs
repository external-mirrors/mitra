use serde::Deserialize;
use uuid::Uuid;

use crate::mastodon_api::{
    pagination::PageSize,
    serializers::deserialize_boolean,
};

fn default_page_size() -> PageSize { PageSize::new(20) }

#[derive(Deserialize)]
pub struct TimelineQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}

fn default_timeline_local() -> bool { false }

#[derive(Deserialize)]
pub struct PublicTimelineQueryParams {
    #[serde(
        default = "default_timeline_local",
        deserialize_with = "deserialize_boolean",
    )]
    pub local: bool,

    pub max_id: Option<Uuid>,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}
