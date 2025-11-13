use serde::Serialize;

use crate::mastodon_api::accounts::types::Account;

#[derive(Serialize)]
pub struct PleromaEmojiReaction {
    pub name: String,
    pub url: Option<String>,
    pub count: i32,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<Account>,

    pub me: bool,
}
