use chrono::Utc;
use uuid::Uuid;

use crate::{
    activitypub::constants::AP_PUBLIC,
    database::DatabaseClient,
    conversations::types::Conversation,
    profiles::types::DbActorProfile,
};

use super::{
    queries::create_post,
    types::{
        ContentType,
        PostContext,
        PostCreateData,
        PostDetailed,
        Visibility,
    },
};

impl PostCreateData {
    pub fn for_test() -> Self {
        Self {
            created_at: Utc::now(), // Default is timestamp 0
            ..Default::default()
        }
    }
}

pub async fn create_test_local_post(
    db_client: &mut impl DatabaseClient,
    author_id: Uuid,
    content: &str,
) -> PostDetailed {
    let post_data = PostCreateData {
        content: content.to_string(),
        content_source: Some(content.to_string()),
        ..PostCreateData::for_test()
    };
    create_post(db_client, author_id, post_data).await.unwrap()
}

pub async fn create_test_remote_post(
    db_client: &mut impl DatabaseClient,
    author_id: Uuid,
    content: &str,
    object_id: &str,
) -> PostDetailed {
    let post_data = PostCreateData {
        content: content.to_string(),
        object_id: Some(object_id.to_string()),
        ..PostCreateData::for_test()
    };
    create_post(db_client, author_id, post_data).await.unwrap()
}

impl Default for PostDetailed {
    fn default() -> Self {
        // TODO: use PostDetailed::new()
        let post_id = Uuid::new_v4();
        Self {
            id: post_id,
            author: DbActorProfile::default(),
            title: None,
            content: "".to_string(),
            content_source: None,
            content_source_type: Some(ContentType::Markdown),
            language: None,
            conversation: Some(Conversation::for_test(post_id)),
            in_reply_to_id: None,
            repost_of_id: None,
            group: None,
            visibility: Visibility::Public,
            is_sensitive: false,
            is_pinned: false,
            reply_count: 0,
            reaction_count: 0,
            repost_count: 0,
            poll: None,
            attachments: vec![],
            mentions: vec![],
            tags: vec![],
            links: vec![],
            emojis: vec![],
            reactions: vec![],
            url: None,
            object_id: None,
            ipfs_cid: None,
            created_at: Utc::now(),
            updated_at: None,
            actions: None,
            related_posts: None,
            parent_visible: true,
        }
    }
}

impl PostDetailed {
    pub fn local_for_test(author: &DbActorProfile) -> Self {
        let post_id = Uuid::new_v4();
        let conversation = Conversation::for_test(post_id);
        Self {
            id: post_id,
            author: author.clone(),
            conversation: Some(conversation),
            ..Default::default()
        }
    }

    pub fn remote_for_test(
        author: &DbActorProfile,
        object_id: &str,
    ) -> Self {
        let post_id = Uuid::new_v4();
        let conversation = Conversation::for_test(post_id);
        Self {
            id: post_id,
            author: author.clone(),
            conversation: Some(conversation),
            object_id: Some(object_id.to_string()),
            ..Default::default()
        }
    }
}

impl Default for PostContext {
    fn default() -> Self {
        Self::Top {
            group_id: None,
            object_id: None,
            audience: Some(AP_PUBLIC.to_owned()),
        }
    }
}

impl PostContext {
    pub fn reply_to(post: &PostDetailed) -> Self {
        Self::Reply {
            conversation_id: post.expect_conversation().id,
            in_reply_to_id: post.id,
        }
    }
}
