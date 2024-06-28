use uuid::Uuid;

use crate::{
    database::DatabaseClient,
    profiles::types::DbActorProfile,
};

use super::{
    queries::create_post,
    types::{Post, PostCreateData},
};

pub async fn create_test_local_post(
    db_client: &mut impl DatabaseClient,
    author_id: Uuid,
    content: &str,
) -> Post {
    let post_data = PostCreateData {
        content: content.to_string(),
        content_source: Some(content.to_string()),
        ..Default::default()
    };
    create_post(db_client, &author_id, post_data).await.unwrap()
}

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
