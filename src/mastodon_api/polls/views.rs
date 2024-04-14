use actix_web::{
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::builders::{
    create_question_vote::prepare_create_question_vote,
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    polls::queries::vote,
    posts::helpers::get_post_by_id_for_view,
};

use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
};
use super::types::{Poll, VoteData};

// https://docs.joinmastodon.org/methods/polls/#vote
#[post("/{poll_id}/votes")]
async fn vote_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    poll_id: web::Path<Uuid>,
    vote_data: web::Json<VoteData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        *poll_id,
    ).await?;
    let poll = post.poll.ok_or(MastodonError::NotFoundError("poll"))?;
    if poll.ended() {
        return Err(MastodonError::OperationError("poll has already ended"));
    };
    if vote_data.choices.is_empty() {
        return Err(MastodonError::OperationError("choice set is empty"));
    };
    if !poll.multiple_choices && vote_data.choices.len() > 1 {
        return Err(MastodonError::OperationError("only one option can be chosen"));
    };
    let (poll_updated, votes) = match vote(
        db_client,
        poll.id,
        current_user.id,
        vote_data.choices.clone(),
    ).await {
        Ok((poll_updated, votes)) => (poll_updated, votes),
        Err(DatabaseError::AlreadyExists(_)) => {
            return Err(MastodonError::OperationError("already voted"));
        },
        Err(other_error) => return Err(other_error.into()),
    };
    if let Some(ref question_id) = post.object_id {
        let question_owner = post.author.expect_actor_data();
        for vote in votes.iter() {
            // Each vote must be sent separately.
            // Pleroma doesn't support Create activities where object is an array.
            prepare_create_question_vote(
                &config.instance(),
                &current_user,
                question_id,
                question_owner,
                vec![vote.clone()],
            )?.save_and_enqueue(db_client).await?;
        };
    };
    let choices = votes.into_iter().map(|vote| vote.choice).collect();
    let poll = Poll::from_db(&poll_updated, Some(choices));
    Ok(HttpResponse::Ok().json(poll))
}

pub fn poll_api_scope() -> Scope {
    web::scope("/v1/polls")
        .service(vote_view)
}
