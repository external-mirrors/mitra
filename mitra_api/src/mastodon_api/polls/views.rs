use actix_web::{
    dev::ConnectionInfo,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::builders::{
    create_question_vote::prepare_create_question_vote,
    update_note::prepare_update_note,
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    polls::queries::vote,
    posts::helpers::{add_related_posts, get_post_by_id_for_view},
    users::queries::get_user_by_id,
};
use mitra_services::media::MediaServer;

use crate::{
    http::get_request_base_url,
    mastodon_api::{
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
    },
};

use super::types::{Poll, VoteData};

// https://docs.joinmastodon.org/methods/polls/#vote
#[post("/{poll_id}/votes")]
async fn vote_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    poll_id: web::Path<Uuid>,
    vote_data: web::Json<VoteData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id_for_view(
        db_client,
        Some(&current_user.profile),
        *poll_id,
    ).await?;
    let poll = post.poll.ok_or(MastodonError::NotFound("poll"))?;
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
    post.poll = Some(poll_updated.clone());
    let instance = config.instance();
    let media_server = MediaServer::new(&config);
    if let Some(ref question_id) = post.object_id {
        // Remote poll
        let question_owner = post.author.expect_actor_data();
        for vote in votes.iter() {
            // Each vote must be sent separately.
            // Pleroma doesn't support Create activities where object is an array.
            prepare_create_question_vote(
                &instance,
                &current_user,
                question_id,
                question_owner,
                vec![vote.clone()],
            )?.save_and_enqueue(db_client).await?;
        };
    } else {
        // Local poll
        let post_author = get_user_by_id(db_client, post.author.id).await?;
        add_related_posts(db_client, vec![&mut post]).await?;
        prepare_update_note(
            db_client,
            &instance,
            &media_server,
            &post_author,
            &post,
        ).await?.save_and_enqueue(db_client).await?;
    };
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let choices = votes.into_iter().map(|vote| vote.choice).collect();
    let poll = Poll::from_db(
        &media_server,
        &poll_updated,
        post.emojis.clone(),
        Some(choices),
    );
    Ok(HttpResponse::Ok().json(poll))
}

pub fn poll_api_scope() -> Scope {
    web::scope("/v1/polls")
        .service(vote_view)
}
