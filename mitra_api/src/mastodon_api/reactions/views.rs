use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    put,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::{
    builders::{
        add_context_activity::sync_conversation,
        like::prepare_like,
        undo_like::prepare_undo_like,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    emojis::queries::get_local_emoji_by_name,
    posts::{
        helpers::get_post_by_id_for_view,
        queries::get_post_reactions,
        types::{PostReaction, Visibility},
    },
    reactions::{
        queries::{
            create_reaction,
            delete_reaction,
            get_reactions,
        },
        types::{ReactionData, ReactionDetailed},
    },
};
use mitra_services::media::MediaServer;
use mitra_utils::unicode::is_single_character;
use mitra_validators::{
    emojis::clean_emoji_name,
    reactions::validate_reaction_data,
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account,
    auth::get_current_user,
    custom_emojis::types::CustomEmoji,
    errors::MastodonError,
    media_server::ClientMediaServer,
    statuses::helpers::build_status,
};

use super::types::PleromaEmojiReaction;

fn emoji_shortcode(emoji_name: &str) -> String {
    format!(":{emoji_name}:")
}

// Pleroma API
// Documentation: https://git.pleroma.social/-/snippets/1945
#[put("/{status_id}/reactions/{content}")]
async fn create_reaction_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    path: web::Path<(Uuid, String)>,
) -> Result<HttpResponse, MastodonError> {
    let (status_id, content) = path.into_inner();
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id_for_view(
        db_client,
        Some(&current_user.profile),
        status_id,
    ).await?;
    let (content, maybe_emoji) = if is_single_character(&content) {
        (content, None)
    } else {
        let emoji_name = clean_emoji_name(&content);
        // Find most popular reaction with matching content
        let maybe_emoji = post.reactions.iter()
            .fold(None, |result: Option<&PostReaction>, item| {
                if item.content == Some(emoji_shortcode(emoji_name)) &&
                    (result.is_none() || item.count > result.map_or(0, |val| val.count))
                {
                    Some(item)
                } else {
                    result
                }
            })
            .and_then(|reaction| reaction.emoji.clone());
        let emoji = if let Some(emoji) = maybe_emoji {
            emoji
        } else {
            get_local_emoji_by_name(db_client, emoji_name).await?
        };
        (emoji.shortcode(), Some(emoji))
    };
    let reaction_data = ReactionData {
        author_id: current_user.id,
        post_id: status_id,
        content: Some(content),
        emoji_id: maybe_emoji.as_ref().map(|db_emoji| db_emoji.id),
        visibility: Visibility::Direct,
        activity_id: None,
    };
    validate_reaction_data(&reaction_data)?;
    let reaction = create_reaction(db_client, reaction_data).await?;
    let reaction = ReactionDetailed
        ::new(reaction, current_user.profile.clone(), maybe_emoji)
        .map_err(DatabaseError::from)?;
    post.reaction_count += 1;
    post.reactions = get_post_reactions(db_client, post.id).await?;
    let media_server = MediaServer::new(&config);
    let like = prepare_like(
        db_client,
        &config.instance(),
        &media_server,
        &current_user,
        &post,
        &reaction,
    ).await?;
    let like_json = like.activity().clone();
    like.save_and_enqueue(db_client).await?;
    sync_conversation(
        db_client,
        &config.instance(),
        post.expect_conversation(),
        like_json,
        reaction.visibility,
    ).await?;

    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let status = build_status(
        db_client,
        config.instance().uri_str(),
        &media_server,
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[delete("/{status_id}/reactions/{content}")]
async fn delete_reaction_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    path: web::Path<(Uuid, String)>,
) -> Result<HttpResponse, MastodonError> {
    let (status_id, content) = path.into_inner();
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id_for_view(
        db_client,
        Some(&current_user.profile),
        status_id,
    ).await?;
    let content = if is_single_character(&content) {
        content
    } else {
        // The value could be a name or a shortcode
        let emoji_name = clean_emoji_name(&content);
        emoji_shortcode(emoji_name)
    };
    let reaction_deleted = delete_reaction(
        db_client,
        current_user.id,
        status_id,
        Some(&content),
    ).await?;
    post.reaction_count -= 1;
    post.reactions = get_post_reactions(db_client, status_id).await?;
    let undo_like = prepare_undo_like(
        db_client,
        &config.instance(),
        &current_user,
        &post,
        reaction_deleted.id,
        reaction_deleted.has_deprecated_ap_id,
    ).await?;
    let undo_like_json = undo_like.activity().clone();
    undo_like.save_and_enqueue(db_client).await?;
    sync_conversation(
        db_client,
        &config.instance(),
        post.expect_conversation(),
        undo_like_json,
        reaction_deleted.visibility,
    ).await?;

    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let status = build_status(
        db_client,
        config.instance().uri_str(),
        &media_server,
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[get("/{status_id}/reactions")]
async fn get_reactions_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id_for_view(
        db_client,
        Some(&current_user.profile),
        *status_id,
    ).await?;
    let reactions = get_reactions(
        db_client,
        post.id,
        Some(current_user.id),
        None,
        None, // get all reactions
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let mut pleroma_reactions: Vec<PleromaEmojiReaction> = vec![];
    for reaction in reactions {
        let Some(content) = reaction.content else {
            // "Favourite"
            continue;
        };
        let maybe_custom_emoji = reaction.emoji
            .map(|emoji| CustomEmoji::from_db(&media_server, emoji));
        let name = maybe_custom_emoji.as_ref()
            .map(|emoji| emoji.shortcode.clone())
            .unwrap_or(content);
        let account = Account::from_profile(
            config.instance().uri_str(),
            &media_server,
            reaction.author.clone(),
        );
        let reacted = reaction.author.id == current_user.id;
        if let Some(item) = pleroma_reactions
            .iter_mut()
            .find(|item| item.name == name)
        {
            item.count += 1;
            item.accounts.push(account);
            if reacted {
                item.me = true;
            };
        } else {
            let item = PleromaEmojiReaction {
                name: name,
                url: maybe_custom_emoji.map(|emoji| emoji.url),
                count: 1,
                accounts: vec![account],
                me: reacted,
            };
            pleroma_reactions.push(item);
        };
    };
    Ok(HttpResponse::Ok().json(pleroma_reactions))
}

pub fn reaction_api_scope() -> Scope {
    web::scope("/v1/pleroma/statuses")
        .service(create_reaction_view)
        .service(delete_reaction_view)
        .service(get_reactions_view)
}
