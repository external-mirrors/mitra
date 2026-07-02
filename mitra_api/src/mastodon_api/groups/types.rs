use serde::Deserialize;

use mitra_models::groups::types::GroupFilter;
use mitra_validators::errors::ValidationError;

use crate::mastodon_api::pagination::PageSize;

#[derive(Deserialize)]
pub struct GroupCreateData {
    pub name: String,
}

const GROUP_FILTER_FOLLOWING: &str = "following";
const GROUP_FILTER_MODERATING: &str = "moderating";

fn default_group_filter() -> String { GROUP_FILTER_FOLLOWING.to_owned() }
fn default_group_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct GroupListQueryParams {
    #[serde(default = "default_group_filter")]
    filter: String,

    #[serde(default)]
    pub offset: u16,

    #[serde(default = "default_group_list_page_size")]
    pub limit: PageSize,
}

impl GroupListQueryParams {
    pub fn filter(&self) -> Result<GroupFilter, ValidationError> {
        let filter = match self.filter.as_str() {
            GROUP_FILTER_FOLLOWING => GroupFilter::Following,
            GROUP_FILTER_MODERATING => GroupFilter::Moderating,
            _ => return Err(ValidationError("invalid filter type")),
        };
        Ok(filter)
    }
}
