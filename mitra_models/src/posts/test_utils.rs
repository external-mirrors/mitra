use crate::profiles::types::DbActorProfile;

use super::types::Post;

impl Post {
    pub fn remote_for_test(
        author: &DbActorProfile,
        object_id: &str,
    ) -> Self {
        Post {
            author: author.clone(),
            object_id: Some(object_id.to_string()),
            ..Default::default()
        }
    }
}
