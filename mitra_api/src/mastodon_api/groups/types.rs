use serde::Deserialize;

use crate::mastodon_api::pagination::PageSize;

fn default_group_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct GroupListQueryParams {
    #[serde(default)]
    pub offset: u16,

    #[serde(default = "default_group_list_page_size")]
    pub limit: PageSize,
}
