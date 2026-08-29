use chrono::Utc;

use crate::profiles::types::DbActorProfile;

use super::types::{EventType, NotificationDetailed};

impl NotificationDetailed {
    pub fn for_test() -> Self {
        Self {
            id: i32::MAX,
            sender: DbActorProfile::default(),
            post: None,
            reaction_content: None,
            reaction_emoji: None,
            payment_amount: None,
            moderation_action: None,
            event_type: EventType::Follow,
            created_at: Utc::now(),
        }
    }
}
