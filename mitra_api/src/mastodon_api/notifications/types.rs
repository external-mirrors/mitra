use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::{
    notifications::types::{
        EventType,
        NotificationDetailed as DbNotificationDetailed,
    },
    profiles::types::MentionPolicy,
    users::types::User,
};

use crate::mastodon_api::{
    accounts::types::Account,
    custom_emojis::types::CustomEmoji,
    media_server::ClientMediaServer,
    pagination::PageSize,
    serializers::serialize_datetime,
    statuses::types::Status,
};

fn default_page_size() -> PageSize { PageSize::new(20) }

// https://docs.joinmastodon.org/methods/notifications/
#[derive(Deserialize)]
pub struct NotificationQueryParams {
    pub min_id: Option<i32>,
    pub max_id: Option<i32>,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}

#[derive(Serialize)]
pub struct EmojiReaction {
    content: String,
    emoji: Option<CustomEmoji>,
}

// https://docs.joinmastodon.org/entities/notification/
#[derive(Serialize)]
pub struct Notification {
    pub id: String,

    #[serde(rename = "type")]
    event_type: String,
    subtype: Option<String>,

    account: Account,
    status: Option<Status>,

    reaction: Option<EmojiReaction>,
    // Pleroma compatibility
    emoji: Option<String>,
    emoji_url: Option<String>,

    #[serde(serialize_with = "serialize_datetime")]
    created_at: DateTime<Utc>,
}

impl Notification {
    pub fn from_db(
        instance_uri: &str,
        media_server: &ClientMediaServer,
        notification: DbNotificationDetailed,
    ) -> Self {
        let account = Account::from_profile(
            instance_uri,
            media_server,
            notification.sender.clone(),
        );
        let status = notification.post.map(|post| {
            Status::from_post(instance_uri, media_server, post)
        });
        let mut maybe_event_subtype = None;
        let event_type_mastodon = match notification.event_type {
            EventType::Follow => "follow",
            EventType::FollowRequest => "follow_request",
            EventType::Reply => {
                maybe_event_subtype = Some("reply".to_string());
                "mention"
            },
            EventType::Reaction if notification.reaction_content.is_none() => "favourite",
            // https://docs.pleroma.social/backend/development/API/differences_in_mastoapi_responses/#emojireact-notification
            EventType::Reaction => "pleroma:emoji_reaction",
            EventType::Mention => "mention",
            EventType::Repost => "reblog",
            EventType::SubscriberPayment if notification.sender.is_anonymous() => "payment_anonymous",
            EventType::SubscriberPayment => "subscription",
            EventType::SubscriptionStart => "", // not supported
            EventType::SubscriptionExpiration => "subscription_expiration",
            EventType::SubscriberLeaving => "subscriber_leaving",
            EventType::Move => "move",
            EventType::SignUp => "admin.sign_up",
        };
        let maybe_reaction = if let Some(content) = notification.reaction_content {
            let maybe_custom_emoji = notification.reaction_emoji
                .map(|emoji| CustomEmoji::from_db(media_server, emoji));
            let reaction = EmojiReaction {
                content,
                emoji: maybe_custom_emoji,
            };
            Some(reaction)
        } else {
            None
        };
        let maybe_emoji_content = maybe_reaction.as_ref()
            .map(|reaction| reaction.content.clone());
        let maybe_emoji_url = maybe_reaction.as_ref().and_then(|reaction| {
            reaction.emoji.as_ref().map(|emoji| emoji.url.clone())
        });
        Self {
            id: notification.id.to_string(),
            event_type: event_type_mastodon.to_string(),
            subtype: maybe_event_subtype,
            account,
            status,
            reaction: maybe_reaction,
            emoji: maybe_emoji_content,
            emoji_url: maybe_emoji_url,
            created_at: notification.created_at,
        }
    }
}

// https://docs.joinmastodon.org/entities/NotificationPolicy/
#[derive(Serialize)]
struct NotificationSummary {
    pending_requests_count: u32,
    pending_notifications_count: u32,
}

#[derive(Serialize)]
pub struct NotificationPolicy {
    for_not_following: &'static str,
    for_not_followers: &'static str,
    for_new_accounts: &'static str,
    for_private_mentions: &'static str,
    for_limited_accounts: &'static str,
    summary: NotificationSummary,
}

impl NotificationPolicy {
    pub fn from_user(user: &User) -> Self {
        const ACCEPT: &str = "accept";
        const DROP: &str = "drop";
        let mention_policy = user.profile.mention_policy;
        Self {
            for_not_following:
                if mention_policy == MentionPolicy::OnlyContacts
                { DROP } else { ACCEPT },
            for_not_followers:
                if mention_policy == MentionPolicy::OnlyContacts
                { DROP } else { ACCEPT },
            for_new_accounts:
                if mention_policy == MentionPolicy::OnlyKnown
                { DROP } else { ACCEPT },
            for_private_mentions: ACCEPT,
            for_limited_accounts: ACCEPT,
            summary: NotificationSummary {
                pending_requests_count: 0,
                pending_notifications_count: 0,
            },
        }
    }
}
