use actix_web::{
    dev::ConnectionInfo,
    http::Uri,
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_activitypub::authority::Authority;
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    conversations::queries::get_direct_conversations,
    posts::helpers::{add_related_posts, add_user_actions},
};

use crate::{
    http::get_request_base_url,
    mastodon_api::{
        auth::get_current_user,
        errors::MastodonError,
        media_server::ClientMediaServer,
        pagination::{get_last_item, get_paginated_response},
    },
};

use super::types::{
    Conversation,
    ConversationListQueryParams,
};

// https://docs.joinmastodon.org/methods/conversations/#get
#[get("")]
async fn get_conversations_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    query_params: web::Query<ConversationListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut db_conversations = get_direct_conversations(
        db_client,
        current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    add_related_posts(
        db_client,
        db_conversations.iter_mut()
            .map(|item| &mut item.last_post)
            .collect(),
    ).await?;
    add_user_actions(
        db_client,
        current_user.id,
        db_conversations.iter_mut()
            .map(|item| &mut item.last_post)
            .collect(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let authority = Authority::from(&config.instance());
    let media_server = ClientMediaServer::new(&config, &base_url);
    let maybe_last_id = get_last_item(&db_conversations, &query_params.limit)
        .map(|db_conversation| db_conversation.conversation.id);
    let conversations: Vec<_> = db_conversations
        .into_iter()
        .map(|db_conversation| Conversation::from_db(
            &authority,
            &media_server,
            db_conversation,
        ))
        .collect();
    let response = get_paginated_response(
        &base_url,
        &request_uri,
        conversations,
        maybe_last_id,
    );
    Ok(response)
}

pub fn conversation_api_scope() -> Scope {
    web::scope("/v1/conversations")
        .service(get_conversations_view)
}
