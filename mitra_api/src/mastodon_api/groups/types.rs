use serde::{Deserialize, Serialize};

use mitra_activitypub::authority::Authority;
use mitra_models::{
    groups::types::GroupFilter,
    relationships::types::{
        RelatedActorProfile,
        RelationshipType,
    },
};
use mitra_validators::errors::ValidationError;

use crate::mastodon_api::{
    accounts::types::Account,
    media_server::ClientMediaServer,
    pagination::PageSize,
};

#[derive(Deserialize)]
pub struct GroupCreateForm {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct GroupSource {
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct GroupUpdateForm {
    pub description: Option<String>,
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

#[derive(Serialize)]
pub struct Affiliation {
    account: Account,
    affiliation: &'static str,
}

impl Affiliation {
    pub fn from_related_profile(
        authority: &Authority,
        media_server: &ClientMediaServer,
        related_profile: RelatedActorProfile<i32>,
    ) -> Self {
        Self {
            account: Account::from_profile(
                authority,
                media_server,
                related_profile.profile,
            ),
            affiliation: match related_profile.relationship_type {
                RelationshipType::GroupAdmin => "admin",
                _ => "none",
            },
        }
    }
}
