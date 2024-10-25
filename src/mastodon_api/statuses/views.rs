/// https://docs.joinmastodon.org/methods/statuses/
use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    post,
    put,
    web,
    Either,
    HttpRequest,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use chrono::Utc;
use uuid::Uuid;

use mitra_activitypub::{
    authority::Authority,
    builders::{
        announce::prepare_announce,
        add_note::prepare_add_note,
        create_note::prepare_create_note,
        delete_note::prepare_delete_note,
        like::prepare_like,
        note::build_note,
        remove_note::prepare_remove_note,
        undo_announce::prepare_undo_announce,
        undo_like::prepare_undo_like,
        update_note::prepare_update_note,
    },
    queues::FetcherJobData,
};
use mitra_config::Config;
use mitra_models::{
    bookmarks::queries::{create_bookmark, delete_bookmark},
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    posts::helpers::{
        add_user_actions,
        can_create_post,
        get_post_by_id_for_view,
    },
    posts::queries::{
        create_post,
        delete_post,
        delete_repost,
        get_post_by_id,
        get_post_reactions,
        get_repost_by_author,
        get_thread,
        set_pinned_flag,
        set_post_ipfs_cid,
        update_post,
    },
    posts::types::{PostCreateData, PostUpdateData, Visibility},
    reactions::queries::{
        create_reaction,
        delete_reaction,
    },
    reactions::types::ReactionData,
    relationships::queries::get_subscribers,
    users::types::Permission,
};
use mitra_services::{
    ipfs::{store as ipfs_store},
    media::MediaStorage,
};
use mitra_validators::{
    errors::ValidationError,
    posts::{
        validate_local_post_links,
        validate_local_reply,
        validate_post_create_data,
        validate_post_mentions,
        validate_post_update_data,
    },
    reactions::validate_reaction_data,
};

use crate::http::{get_request_base_url, QsFormOrJson};
use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
};
use crate::state::AppState;

use super::helpers::{
    build_status,
    build_status_list,
    parse_content,
    PostContent,
};
use super::types::{
    Context,
    Status,
    StatusData,
    StatusPreview,
    StatusPreviewData,
    StatusSource,
    StatusUpdateData,
};

#[post("")]
async fn create_status(
    app_state: web::Data<AppState>,
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request: HttpRequest,
    status_data: QsFormOrJson<StatusData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !can_create_post(&current_user) {
        return Err(MastodonError::PermissionError);
    };
    let instance = config.instance();
    let status_data = match status_data {
        Either::Left(form) => form.into_inner(),
        Either::Right(json) => json.into_inner(),
    };
    let maybe_in_reply_to = if let Some(in_reply_to_id) = status_data.in_reply_to_id {
        let in_reply_to = match get_post_by_id_for_view(
            db_client,
            Some(&current_user),
            in_reply_to_id,
        ).await {
            Ok(post) => post,
            Err(DatabaseError::NotFound(_)) => {
                return Err(ValidationError("parent post does not exist").into());
            },
            Err(other_error) => return Err(other_error.into()),
        };
        Some(in_reply_to)
    } else {
        None
    };
    let visibility = match status_data.visibility.as_deref() {
        Some("public" | "unlisted") => Visibility::Public,
        Some("direct") => Visibility::Direct,
        Some("private") => Visibility::Followers,
        Some("subscribers") => Visibility::Subscribers,
        Some(_) => return Err(ValidationError("invalid visibility parameter").into()),
        None => {
            maybe_in_reply_to.as_ref()
                .map(|post| match post.visibility {
                    Visibility::Public => Visibility::Public,
                    _ => Visibility::Direct,
                })
                .unwrap_or(Visibility::Public)
        },
    };
    // Parse content
    let PostContent { content, content_source, mut mentions, hashtags, links, linked, emojis } =
        parse_content(
            db_client,
            &instance,
            &status_data.status,
            &status_data.content_type,
            status_data.quote_id,
        ).await?;

    // Extend mentions
    if let Some(ref in_reply_to) = maybe_in_reply_to {
        // Always mention the author of the parent post
        mentions.push(in_reply_to.author.id);
    };
    if visibility == Visibility::Subscribers {
        // Mention all subscribers.
        // This makes post accessible only to active subscribers
        // and is required for sending activities to subscribers
        // on other instances.
        let subscribers = get_subscribers(db_client, current_user.id).await?
            .into_iter().map(|profile| profile.id);
        mentions.extend(subscribers);
    };
    // Remove duplicate mentions
    mentions.sort();
    mentions.dedup();

    // Validate post data
    let post_data = PostCreateData {
        content: content,
        content_source: content_source,
        in_reply_to_id: status_data.in_reply_to_id,
        repost_of_id: None,
        visibility: visibility,
        is_sensitive: status_data.sensitive,
        attachments: status_data.media_ids.iter().copied()
            .chain(status_data.media_ids_json.iter().copied())
            .collect(),
        mentions: mentions,
        tags: hashtags,
        links: links,
        emojis: emojis.iter().map(|emoji| emoji.id).collect(),
        object_id: None,
        created_at: Utc::now(),
    };
    validate_post_create_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, &post_data.visibility)?;
    validate_local_post_links(&post_data.links, &post_data.visibility)?;
    if let Some(ref in_reply_to) = maybe_in_reply_to {
        validate_local_reply(in_reply_to, &post_data.mentions, &post_data.visibility)?;
    };

    // Check idempotency key
    // https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/
    let maybe_idempotency_key = request.headers()
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let mut post_id_cache = app_state.post_id_cache.lock().await;
    if let Some(ref idempotency_key) = maybe_idempotency_key {
        if let Some(post_id) = post_id_cache.get(idempotency_key) {
            log::warn!("idempotency key re-used: {idempotency_key}");
            // TODO: store Uuid in cache
            let post_id = Uuid::parse_str(post_id)
                .map_err(|_| MastodonError::InternalError)?;
            let post = get_post_by_id(db_client, post_id).await?;
            if post.author.id != current_user.id {
                return Err(MastodonError::PermissionError);
            };
            let status = build_status(
                db_client,
                &get_request_base_url(connection_info),
                &instance.url(),
                Some(&current_user),
                post,
            ).await?;
            return Ok(HttpResponse::Ok().json(status));
        };
    };

    // Create post
    let mut post = create_post(db_client, current_user.id, post_data).await?;
    if let Some(idempotency_key) = maybe_idempotency_key {
        post_id_cache.set(idempotency_key, post.id.to_string());
    };
    drop(post_id_cache); // release lock

    // Same as add_related_posts
    post.in_reply_to = maybe_in_reply_to.map(|mut in_reply_to| {
        in_reply_to.reply_count += 1;
        Box::new(in_reply_to)
    });
    post.linked = linked;
    // Federate
    prepare_create_note(
        db_client,
        &instance,
        &current_user,
        &post,
        config.federation.fep_e232_enabled,
    ).await?.save_and_enqueue(db_client).await?;

    let status = Status::from_post(
        &get_request_base_url(connection_info),
        &instance.url(),
        post,
    );
    Ok(HttpResponse::Ok().json(status))
}

#[post("/preview")]
async fn preview_status(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_data: web::Json<StatusPreviewData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let instance = config.instance();
    let status_data = status_data.into_inner();
    let PostContent { content, emojis, .. } =
        parse_content(
            db_client,
            &instance,
            &status_data.status,
            &status_data.content_type,
            None,
        ).await?;
    // Return preview
    let preview = StatusPreview::new(
        &instance.url(),
        content,
        emojis,
    );
    Ok(HttpResponse::Ok().json(preview))
}

#[get("/{status_id}")]
async fn get_status(
    auth: Option<BearerAuth>,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let post = get_post_by_id_for_view(
        db_client,
        maybe_current_user.as_ref(),
        *status_id,
    ).await?;
    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        maybe_current_user.as_ref(),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[get("/{status_id}/source")]
async fn get_status_source(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id(db_client, *status_id).await?;
    if post.author.id != current_user.id {
        return Err(MastodonError::PermissionError);
    };
    let status_source = StatusSource::from_post(post);
    Ok(HttpResponse::Ok().json(status_source))
}

#[put("/{status_id}")]
async fn edit_status(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
    status_data: web::Json<StatusUpdateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id(db_client, *status_id).await?;
    if post.author.id != current_user.id {
        return Err(MastodonError::PermissionError);
    };
    let maybe_in_reply_to = if let Some(in_reply_to_id) = post.in_reply_to_id {
        let in_reply_to = get_post_by_id(db_client, in_reply_to_id).await?;
        Some(in_reply_to)
    } else {
        None
    };
    let instance = config.instance();
    let status_data = status_data.into_inner();
    // Parse content
    let PostContent { content, content_source, mut mentions, hashtags, links, linked, emojis } =
        parse_content(
            db_client,
            &instance,
            &status_data.status,
            &status_data.content_type,
            status_data.quote_id,
        ).await?;

    // Extend mentions
    if post.visibility == Visibility::Subscribers {
        // Mention all subscribers.
        // This makes post accessible only to active subscribers
        // and is required for sending activities to subscribers
        // on other instances.
        let subscribers = get_subscribers(db_client, current_user.id).await?
            .into_iter().map(|profile| profile.id);
        mentions.extend(subscribers);
    };
    // Remove duplicate mentions
    mentions.sort();
    mentions.dedup();

    // Update post
    let post_data = PostUpdateData {
        content: content,
        content_source: content_source,
        is_sensitive: status_data.sensitive,
        attachments: status_data.media_ids.unwrap_or(vec![]),
        mentions: mentions,
        tags: hashtags,
        links: links,
        emojis: emojis.iter().map(|emoji| emoji.id).collect(),
        updated_at: Utc::now(),
    };
    validate_post_update_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, &post.visibility)?;
    validate_local_post_links(&post_data.links, &post.visibility)?;
    if let Some(ref in_reply_to) = maybe_in_reply_to {
        validate_local_reply(in_reply_to, &post_data.mentions, &post.visibility)?;
    };
    let (mut post, deletion_queue) =
        update_post(db_client, post.id, post_data).await?;
    deletion_queue.into_job(db_client).await?;
    // Same as add_related_posts
    post.in_reply_to = maybe_in_reply_to.map(Box::new);
    post.linked = linked;
    add_user_actions(db_client, current_user.id, vec![&mut post]).await?;

    // Federate
    prepare_update_note(
        db_client,
        &instance,
        &current_user,
        &post,
        config.federation.fep_e232_enabled,
    ).await?.save_and_enqueue(db_client).await?;

    let status = Status::from_post(
        &get_request_base_url(connection_info),
        &instance.url(),
        post,
    );
    Ok(HttpResponse::Ok().json(status))
}

#[delete("/{status_id}")]
async fn delete_status(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id(db_client, *status_id).await?;
    if post.author.id != current_user.id {
        return Err(MastodonError::PermissionError);
    };
    let delete_note = prepare_delete_note(
        db_client,
        &config.instance(),
        &current_user,
        &post,
        config.federation.fep_e232_enabled,
    ).await?;
    let deletion_queue = delete_post(db_client, *status_id).await?;
    deletion_queue.into_job(db_client).await?;
    delete_note.save_and_enqueue(db_client).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[get("/{status_id}/context")]
async fn get_context(
    auth: Option<BearerAuth>,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let posts = get_thread(
        db_client,
        *status_id,
        maybe_current_user.as_ref().map(|user| user.id),
    ).await?;
    let statuses = build_status_list(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        maybe_current_user.as_ref(),
        posts,
    ).await?;
    let mut ancestors = vec![];
    let mut descendants = vec![];
    let mut is_ancestor = true;
    for status in statuses {
        if is_ancestor {
            if status.id == *status_id {
                is_ancestor = false;
                continue;
            };
            ancestors.push(status);
        } else {
            descendants.push(status);
        };
    };
    let context = Context { ancestors, descendants };
    Ok(HttpResponse::Ok().json(context))
}

#[get("/{status_id}/thread")]
async fn get_thread_view(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let posts = get_thread(
        db_client,
        *status_id,
        maybe_current_user.as_ref().map(|user| user.id),
    ).await?;
    let statuses = build_status_list(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        maybe_current_user.as_ref(),
        posts,
    ).await?;
    Ok(HttpResponse::Ok().json(statuses))
}

#[post("/{status_id}/favourite")]
async fn favourite(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        *status_id,
    ).await?;
    let reaction_data = ReactionData {
        author_id: current_user.id,
        post_id: status_id.into_inner(),
        content: None,
        emoji_id: None,
        activity_id: None,
    };
    validate_reaction_data(&reaction_data)?;
    let maybe_reaction_created = match create_reaction(
        db_client, reaction_data,
    ).await {
        Ok(reaction) => {
            post.reaction_count += 1;
            post.reactions = get_post_reactions(db_client, post.id).await?;
            Some(reaction)
        },
        Err(DatabaseError::AlreadyExists(_)) => None, // post already favourited
        Err(other_error) => return Err(other_error.into()),
    };

    if let Some(reaction) = maybe_reaction_created {
        // Federate
        prepare_like(
            db_client,
            &config.instance(),
            &current_user,
            &post,
            reaction.id,
            None,
            None,
        ).await?.save_and_enqueue(db_client).await?;
    };

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[post("/{status_id}/unfavourite")]
async fn unfavourite(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        *status_id,
    ).await?;
    let maybe_reaction_deleted = match delete_reaction(
        db_client,
        current_user.id,
        *status_id,
        None, // not an emoji reaction
    ).await {
        Ok(reaction_deleted) => {
            post.reaction_count -= 1;
            post.reactions = get_post_reactions(db_client, post.id).await?;
            Some(reaction_deleted)
        },
        Err(DatabaseError::NotFound(_)) => None, // post not favourited
        Err(other_error) => return Err(other_error.into()),
    };

    if let Some((reaction_id, reaction_has_deprecated_ap_id)) = maybe_reaction_deleted {
        // Federate
        prepare_undo_like(
            db_client,
            &config.instance(),
            &current_user,
            &post,
            reaction_id,
            reaction_has_deprecated_ap_id,
        ).await?.save_and_enqueue(db_client).await?;
    };

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[post("/{status_id}/reblog")]
async fn reblog(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !can_create_post(&current_user) {
        return Err(MastodonError::PermissionError);
    };
    let mut post = get_post_by_id(db_client, *status_id).await?;
    if !post.is_public() {
        return Err(MastodonError::NotFoundError("post"));
    };
    let repost_data = PostCreateData::repost(status_id.into_inner(), None);
    let mut repost = create_post(db_client, current_user.id, repost_data).await?;
    post.repost_count += 1;
    repost.repost_of = Some(Box::new(post));

    // Federate
    prepare_announce(
        db_client,
        &config.instance(),
        &current_user,
        &repost,
    ).await?.save_and_enqueue(db_client).await?;

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        repost,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[post("/{status_id}/unreblog")]
async fn unreblog(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let (repost_id, repost_has_deprecated_ap_id) = get_repost_by_author(
        db_client,
        *status_id,
        current_user.id,
    ).await?;
    delete_repost(db_client, repost_id).await?;
    let post = get_post_by_id(db_client, *status_id).await?;

    // Federate
    prepare_undo_announce(
        db_client,
        &config.instance(),
        &current_user,
        &post,
        repost_id,
        repost_has_deprecated_ap_id,
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

/// https://docs.joinmastodon.org/methods/statuses/#bookmark
#[post("/{status_id}/bookmark")]
async fn bookmark_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        *status_id,
    ).await?;
    match create_bookmark(db_client, current_user.id, post.id).await {
        Ok(_) | Err(DatabaseError::AlreadyExists(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

/// https://docs.joinmastodon.org/methods/statuses/#unbookmark
#[post("/{status_id}/unbookmark")]
async fn unbookmark_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        *status_id,
    ).await?;
    match delete_bookmark(db_client, current_user.id, post.id).await {
        Ok(_) | Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

/// https://docs.joinmastodon.org/methods/statuses/#pin
#[post("/{status_id}/pin")]
async fn pin(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, *status_id).await?;
    if post.author.id != current_user.id || !post.is_public() {
        return Err(MastodonError::OperationError("can't pin post"));
    };
    set_pinned_flag(db_client, post.id, true).await?;
    post.is_pinned = true;

    prepare_add_note(
        db_client,
        &config.instance(),
        &current_user,
        post.id,
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

/// https://docs.joinmastodon.org/methods/statuses/#unpin
#[post("/{status_id}/unpin")]
async fn unpin(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, *status_id).await?;
    if post.author.id != current_user.id || !post.is_public() {
        return Err(MastodonError::OperationError("can't unpin post"));
    };
    set_pinned_flag(db_client, post.id, false).await?;
    post.is_pinned = false;

    prepare_remove_note(
        db_client,
        &config.instance(),
        &current_user,
        post.id,
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

#[post("/{status_id}/make_permanent")]
async fn make_permanent(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, *status_id).await?;
    if post.ipfs_cid.is_some() {
        return Err(MastodonError::OperationError("post already saved to IPFS"));
    };
    if post.author.id != current_user.id || !post.is_public() {
        // Users can only archive their own public posts
        return Err(MastodonError::PermissionError);
    };
    let ipfs_api_url = config.ipfs_api_url.as_ref()
        .ok_or(MastodonError::NotSupported)?;
    let media_storage = MediaStorage::from(config.as_ref());

    let mut attachments = vec![];
    for attachment in post.attachments.iter_mut() {
        // Add attachment to IPFS
        let image_data = media_storage.read_file(&attachment.file_name)
            .map_err(|_| MastodonError::InternalError)?;
        let image_cid = ipfs_store::add(ipfs_api_url, image_data).await
            .map_err(|_| MastodonError::InternalError)?;
        attachment.ipfs_cid = Some(image_cid.clone());
        attachments.push((attachment.id, image_cid));
    };
    assert!(post.is_local());
    let instance = config.instance();
    let authority = Authority::server(&instance.url());
    let note = build_note(
        &instance.hostname(),
        &instance.url(),
        &authority,
        &post,
        config.federation.fep_e232_enabled,
        true,
    );
    let post_metadata = serde_json::to_value(note)
        .map_err(|_| MastodonError::InternalError)?;
    let post_metadata_json = post_metadata.to_string().as_bytes().to_vec();
    let post_metadata_cid = ipfs_store::add(ipfs_api_url, post_metadata_json).await
        .map_err(|_| MastodonError::InternalError)?;

    set_post_ipfs_cid(db_client, post.id, &post_metadata_cid, attachments).await?;
    post.ipfs_cid = Some(post_metadata_cid);

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[post("/{status_id}/load_conversation")]
async fn load_conversation(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::DeleteAnyProfile) {
        return Err(MastodonError::PermissionError);
    };
    let post = get_post_by_id_for_view(
        db_client,
        Some(&current_user),
        *status_id,
    ).await?;
    let job_data = if let Some(object_id) = post.object_id {
        FetcherJobData::Context { object_id }
    } else {
        // Local posts
        return Err(MastodonError::NotFoundError("post"));
    };
    job_data.into_job(db_client).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn status_api_scope() -> Scope {
    web::scope("/v1/statuses")
        // Routes without status ID
        .service(create_status)
        .service(preview_status)
        // Routes with status ID
        .service(get_status)
        .service(get_status_source)
        .service(edit_status)
        .service(delete_status)
        .service(get_context)
        .service(get_thread_view)
        .service(favourite)
        .service(unfavourite)
        .service(reblog)
        .service(unreblog)
        .service(pin)
        .service(unpin)
        .service(bookmark_view)
        .service(unbookmark_view)
        .service(make_permanent)
        .service(load_conversation)
}
