use uuid::Uuid;

use super::types::{
    Conversation,
    AP_PUBLIC,
};

impl Conversation {
    pub fn for_test(root_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            root_id,
            audience: Some(AP_PUBLIC.to_owned()),
        }
    }
}
