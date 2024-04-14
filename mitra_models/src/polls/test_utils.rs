use chrono::Utc;
use uuid::Uuid;

use crate::{
    database::DatabaseClient,
    posts::{
        queries::create_post,
        types::{Post, PostCreateData},
    },
};

use super::{
    types::{PollData, PollResult},
};

pub async fn create_test_local_poll(
    db_client: &mut impl DatabaseClient,
    author_id: Uuid,
    options: &[&str],
) -> Post {
    let results = options.iter()
        .map(|name| PollResult {
            option_name: name.to_string(),
            vote_count: 0,
        })
        .collect();
    let poll_data = PollData {
        multiple_choices: false,
        ends_at: Utc::now(),
        results: results,
    };
    let post_data = PostCreateData {
        content: "poll".to_string(),
        content_source: Some("poll".to_string()),
        poll: Some(poll_data),
        ..Default::default()
    };
    create_post(db_client, author_id, post_data).await.unwrap()
}
