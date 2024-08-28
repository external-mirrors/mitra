/// Undocumented Pleroma API
use actix_web::{
    delete,
    dev::ConnectionInfo,
    put,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::{
    builders::{
        like::prepare_like,
        undo_like::prepare_undo_like,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
    emojis::queries::get_local_emoji_by_name,
    posts::helpers::get_post_by_id_for_view,
    posts::queries::get_post_reactions,
    posts::types::DbPostReactions,
    reactions::queries::{
        create_reaction,
        delete_reaction,
    },
    reactions::types::ReactionData,
};
use mitra_utils::unicode::is_single_character;
use mitra_validators::{
    emojis::clean_emoji_name,
    reactions::validate_reaction_data,
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
    statuses::helpers::build_status,
};

fn emoji_shortcode(emoji_name: &str) -> String {
    format!(":{emoji_name}:")
}

#[put("/{status_id}/reactions/{content}")]
async fn create_reaction_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    path: web::Path<(Uuid, String)>,
) -> Result<HttpResponse, MastodonError> {
    let (status_id, content) = path.into_inner();
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        status_id,
    ).await?;
    let (content, maybe_emoji) = if is_single_character(&content) {
        (content, None)
    } else {
        let emoji_name = clean_emoji_name(&content);
        // Find most popular reaction with matching content
        let maybe_emoji = post.reactions.iter()
            .fold(None, |result: Option<&DbPostReactions>, item| {
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
        activity_id: None,
    };
    validate_reaction_data(&reaction_data)?;
    let reaction = create_reaction(db_client, reaction_data).await?;
    post.reaction_count += 1;
    post.reactions = get_post_reactions(db_client, post.id).await?;
    prepare_like(
        db_client,
        &config.instance(),
        &current_user,
        &post,
        reaction.id,
        reaction.content.clone(),
        maybe_emoji.as_ref(),
    ).await?.save_and_enqueue(db_client).await?;

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[delete("/{status_id}/reactions/{content}")]
async fn delete_reaction_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    path: web::Path<(Uuid, String)>,
) -> Result<HttpResponse, MastodonError> {
    let (status_id, content) = path.into_inner();
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        status_id,
    ).await?;
    let content = if is_single_character(&content) {
        content
    } else {
        // The value could be a name or a shortcode
        let emoji_name = clean_emoji_name(&content);
        emoji_shortcode(emoji_name)
    };
    let (reaction_id, reaction_has_deprecated_ap_id) = delete_reaction(
        db_client,
        current_user.id,
        status_id,
        Some(&content),
    ).await?;
    post.reaction_count -= 1;
    post.reactions = get_post_reactions(db_client, status_id).await?;
    prepare_undo_like(
        db_client,
        &config.instance(),
        &current_user,
        &post,
        reaction_id,
        reaction_has_deprecated_ap_id,
    ).await?.save_and_enqueue(db_client).await?;

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

pub fn reaction_api_scope() -> Scope {
    web::scope("/v1/pleroma/statuses")
        .service(create_reaction_view)
        .service(delete_reaction_view)
}
