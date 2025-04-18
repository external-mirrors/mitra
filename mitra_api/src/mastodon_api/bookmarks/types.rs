use serde::Deserialize;

use crate::mastodon_api::{
    pagination::PageSize,
};

fn default_page_size() -> PageSize { PageSize::new(20) }

#[derive(Deserialize)]
pub struct BookmarkListQueryParams {
    pub max_id: Option<i32>,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}
