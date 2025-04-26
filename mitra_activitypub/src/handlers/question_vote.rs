use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    polls::queries::vote_one,
    posts::helpers::{
        add_related_posts,
        can_view_post,
        get_local_post_by_id,
    },
    users::queries::get_user_by_id,
};
use mitra_services::media::MediaServer;
use mitra_validators::errors::ValidationError;

use crate::{
    builders::update_note::prepare_update_note,
    identifiers::parse_local_object_id,
    importers::{
        ActorIdResolver,
        ApClient,
    },
    ownership::verify_object_owner,
    vocabulary::NOTE,
};

use super::{Descriptor, HandlerResult};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuestionVote {
    id: String,
    attributed_to: String,
    in_reply_to: String,
    name: String,
}

pub fn is_question_vote(object: &JsonValue) -> bool {
    let is_vote = {
        object["type"].as_str() == Some(NOTE) &&
        object["inReplyTo"].is_string() &&
        object["name"].is_string() && (
            object["content"].is_null() ||
            // Workaround for Streams
            object["content"].as_str() == Some("")
        )
    };
    if is_vote && !object["content"].is_null() {
        log::warn!("vote content is not null");
    };
    is_vote
}

pub async fn handle_question_vote(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    object: JsonValue,
) -> HandlerResult {
    verify_object_owner(&object)?;
    let vote: QuestionVote = serde_json::from_value(object)?;
    let ap_client = ApClient::new(config, db_client).await?;
    let instance = &ap_client.instance;
    let media_server = MediaServer::new(config);
    let voter = ActorIdResolver::default().only_remote().resolve(
        &ap_client,
        db_client,
        &vote.attributed_to,
    ).await?;
    let post_id = parse_local_object_id(&instance.url(), &vote.in_reply_to)?;
    let mut post = get_local_post_by_id(db_client, post_id).await?;
    if !can_view_post(db_client, Some(&voter), &post).await? {
        log::error!("private post access violation: {}", vote.in_reply_to);
        return Err(ValidationError("actor is not allowed to vote").into());
    };
    let poll = post.poll.ok_or(ValidationError("post doesn't have a poll"))?;
    if poll.ended() {
        return Err(ValidationError("poll has already ended").into());
    };
    let poll_updated = match vote_one(
        db_client,
        poll.id,
        voter.id,
        &vote.name,
        &vote.id,
    ).await {
        Ok(poll_updated) => poll_updated,
        Err(DatabaseError::AlreadyExists(_)) => {
            log::warn!("vote already registered: {}", vote.id);
            return Ok(None);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // Notify poll audience about results
    post.poll = Some(poll_updated);
    add_related_posts(db_client, vec![&mut post]).await?;
    let post_author = get_user_by_id(db_client, post.author.id).await?;
    prepare_update_note(
        db_client,
        instance,
        &media_server,
        &post_author,
        &post,
        config.federation.fep_e232_enabled,
    ).await?.save_and_enqueue(db_client).await?;
    Ok(Some(Descriptor::object("Vote")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_is_question_vote() {
        let object = json!({
            "id": "https://social.example/objects/123",
            "type": "Note",
            "attributedTo": "https://social.example/users/1",
            "name": "test",
            "inReplyTo": "https://social.example/objects/121",
        });
        assert_eq!(is_question_vote(&object), true);
    }

    #[test]
    fn test_is_question_vote_with_content() {
        let object = json!({
            "id": "https://social.example/objects/123",
            "type": "Note",
            "attributedTo": "https://social.example/users/1",
            "content": "test",
            "inReplyTo": "https://social.example/objects/121",
        });
        assert_eq!(is_question_vote(&object), false);
    }
}
