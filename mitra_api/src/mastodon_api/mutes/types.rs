use serde::Deserialize;

use crate::mastodon_api::pagination::PageSize;

fn default_mute_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct MuteListQueryParams {
    pub max_id: Option<i32>,

    #[serde(default = "default_mute_list_page_size")]
    pub limit: PageSize,
}
