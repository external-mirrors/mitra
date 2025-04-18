use serde::Deserialize;
use uuid::Uuid;

use crate::mastodon_api::pagination::PageSize;

fn default_request_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct RequestListQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_request_list_page_size")]
    pub limit: PageSize,
}
