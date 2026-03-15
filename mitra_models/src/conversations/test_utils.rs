use uuid::Uuid;

use crate::activitypub::constants::AP_PUBLIC;
use super::types::Conversation;

impl Conversation {
    pub fn for_test(root_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            root_id,
            audience: Some(AP_PUBLIC.to_owned()),
        }
    }
}
