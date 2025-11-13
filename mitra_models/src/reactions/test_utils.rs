use uuid::Uuid;

use crate::{
    database::DatabaseClient,
    posts::types::Visibility,
};

use super::{
    queries::create_reaction,
    types::{Reaction, ReactionData},
};

pub async fn create_test_local_reaction(
    db_client: &mut impl DatabaseClient,
    author_id: Uuid,
    post_id: Uuid,
    maybe_content: Option<&str>,
) -> Reaction {
    let reaction_data = ReactionData {
        author_id,
        post_id,
        content: maybe_content.map(|content| content.to_owned()),
        emoji_id: None,
        visibility: Visibility::Direct,
        activity_id: None,
    };
    create_reaction(db_client, reaction_data).await.unwrap()
}
