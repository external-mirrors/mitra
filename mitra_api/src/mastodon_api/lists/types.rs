use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_models::{
    custom_feeds::types::DbCustomFeed,
};

use crate::mastodon_api::{
    pagination::PageSize,
};

/// https://docs.joinmastodon.org/entities/List/
#[derive(Serialize)]
pub struct List {
    id: i32,
    title: String,
    replies_policy: String,
    exclusive: bool,
}

impl List {
    pub fn from_db(db_feed: DbCustomFeed) -> Self {
        Self {
            id: db_feed.id,
            title: db_feed.feed_name,
            // "Show replies to any followed user"
            replies_policy: "followed".to_string(),
            // All custom feeds are "exclusive"
            exclusive: true,
        }
    }
}

#[derive(Deserialize)]
pub struct ListData {
    pub title: String,
}

fn default_list_accounts_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct ListAccountsQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_list_accounts_page_size")]
    pub limit: PageSize,
}

#[derive(Deserialize)]
pub struct ListAccountsData {
    #[serde(alias = "account_ids[]")]
    pub account_ids: Vec<Uuid>,
}
