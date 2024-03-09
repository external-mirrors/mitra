use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::notifications::types::{EventType, Notification};

use crate::mastodon_api::{
    accounts::types::Account,
    custom_emojis::types::CustomEmoji,
    pagination::PageSize,
    statuses::types::Status,
};

fn default_page_size() -> PageSize { PageSize::new(20) }

/// https://docs.joinmastodon.org/methods/notifications/
#[derive(Deserialize)]
pub struct NotificationQueryParams {
    pub max_id: Option<i32>,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}

#[derive(Serialize)]
pub struct EmojiReaction {
    content: String,
    emoji: Option<CustomEmoji>,
}

/// https://docs.joinmastodon.org/entities/notification/
#[derive(Serialize)]
pub struct ApiNotification {
    pub id: String,

    #[serde(rename = "type")]
    pub event_type: String,

    pub account: Account,
    pub status: Option<Status>,
    pub reaction: Option<EmojiReaction>,
    pub created_at: DateTime<Utc>,
}

impl ApiNotification {
    pub fn from_db(
        base_url: &str,
        instance_url: &str,
        notification: Notification,
    ) -> Self {
        let account = Account::from_profile(
            base_url,
            instance_url,
            notification.sender,
        );
        let status = notification.post.map(|post| {
            Status::from_post(base_url, instance_url, post)
        });
        let event_type_mastodon = match notification.event_type {
            EventType::Follow => "follow",
            EventType::FollowRequest => "follow_request",
            EventType::Reply => "reply",
            EventType::Reaction if notification.reaction_content.is_none() => "favourite",
            EventType::Reaction => "emoji_reaction",
            EventType::Mention => "mention",
            EventType::Repost => "reblog",
            EventType::Subscription => "subscription",
            EventType::SubscriptionStart => "", // not supported
            EventType::SubscriptionExpiration => "subscription_expiration",
            EventType::Move => "move",
            EventType::SignUp => "admin.sign_up",
        };
        let maybe_reaction = if let Some(content) = notification.reaction_content {
            let maybe_custom_emoji = notification.reaction_emoji
                .map(|emoji| CustomEmoji::from_db(base_url, emoji));
            let reaction = EmojiReaction {
                content,
                emoji: maybe_custom_emoji,
            };
            Some(reaction)
        } else {
            None
        };
        Self {
            id: notification.id.to_string(),
            event_type: event_type_mastodon.to_string(),
            account,
            status,
            reaction: maybe_reaction,
            created_at: notification.created_at,
        }
    }
}
