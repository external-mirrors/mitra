use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_activitypub::authority::Authority;
use mitra_models::{
    conversations::types::{ConversationPreview as DbConversationPreview},
};

use crate::mastodon_api::{
    accounts::types::Account,
    media_server::ClientMediaServer,
    pagination::PageSize,
    statuses::types::Status,
};

// https://docs.joinmastodon.org/entities/Conversation/
#[derive(Serialize)]
pub struct Conversation {
    id: Uuid,
    unread: bool,
    accounts: Vec<Account>,
    last_status: Status,
}

impl Conversation {
    pub fn from_db(
        authority: &Authority,
        media_server: &ClientMediaServer,
        db_conversation_preview: DbConversationPreview,
    ) -> Self {
        let accounts = db_conversation_preview.participants
            .into_iter()
            .map(|profile| Account::from_profile(
                authority,
                media_server,
                profile,
            ))
            .collect();
        let last_status = Status::from_post(
            authority,
            media_server,
            db_conversation_preview.last_post,
        );
        Self {
            id: db_conversation_preview.conversation.id,
            unread: false,
            accounts,
            last_status,
        }
    }
}

fn default_page_size() -> PageSize { PageSize::new(20) }

#[derive(Deserialize)]
pub struct ConversationListQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}
