/// https://docs.joinmastodon.org/methods/statuses/
use actix_web::{
    delete,
    dev::ConnectionInfo,
    get,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseError, DbPool},
    posts::helpers::{can_create_post, can_view_post},
    posts::queries::{
        create_post,
        delete_post,
        get_post_by_id,
        get_repost_by_author,
        get_thread,
        set_pinned_flag,
        set_post_ipfs_cid,
    },
    posts::types::{PostCreateData, Visibility},
    reactions::queries::{
        create_reaction,
        delete_reaction,
    },
    relationships::queries::get_subscribers,
};
use mitra_services::ipfs::{store as ipfs_store};
use mitra_utils::markdown::markdown_lite_to_html;
use mitra_validators::{
    errors::ValidationError,
    posts::{
        clean_local_content,
        validate_post_create_data,
        validate_post_mentions,
    },
};

use crate::activitypub::{
    builders::{
        announce::prepare_announce,
        add_note::prepare_add_note,
        create_note::{build_note, prepare_create_note},
        delete_note::prepare_delete_note,
        like::prepare_like,
        remove_note::prepare_remove_note,
        undo_announce::prepare_undo_announce,
        undo_like::prepare_undo_like,
    },
    identifiers::local_object_id,
};
use crate::http::{get_request_base_url, FormOrJson};
use crate::mastodon_api::{
    errors::MastodonError,
    oauth::auth::get_current_user,
};
use crate::media::{read_file, remove_media};

use super::helpers::{
    build_status,
    build_status_list,
    parse_microsyntaxes,
    PostContent,
};
use super::types::{
    Context,
    Status,
    StatusData,
    StatusPreview,
    StatusPreviewData,
};

#[post("")]
async fn create_status(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    status_data: FormOrJson<StatusData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !can_create_post(&current_user) {
        return Err(MastodonError::PermissionError);
    };
    let instance = config.instance();
    let status_data = status_data.into_inner();
    let maybe_in_reply_to = if let Some(in_reply_to_id) = status_data.in_reply_to_id.as_ref() {
        let in_reply_to = match get_post_by_id(db_client, in_reply_to_id).await {
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
    let (content, maybe_content_source) = match status_data.content_type.as_str() {
        "text/html" => (status_data.status, None),
        "text/markdown" => {
            let content = markdown_lite_to_html(&status_data.status)
                .map_err(|_| ValidationError("invalid markdown"))?;
            (content, Some(status_data.status))
        },
        _ => return Err(ValidationError("unsupported content type").into()),
    };
    // Parse content
    let PostContent { mut content, mut mentions, hashtags, links, linked, emojis } =
        parse_microsyntaxes(
            db_client,
            &instance,
            content,
        ).await?;
    // Clean content
    content = clean_local_content(&content)?;

    // Extend mentions
    if visibility == Visibility::Subscribers {
        // Mention all subscribers.
        // This makes post accessible only to active subscribers
        // and is required for sending activities to subscribers
        // on other instances.
        let subscribers = get_subscribers(db_client, &current_user.id).await?
            .into_iter().map(|profile| profile.id);
        mentions.extend(subscribers);
    };
    // Remove duplicate mentions
    mentions.sort();
    mentions.dedup();

    // Links validation
    if links.len() > 0 && visibility != Visibility::Public {
        return Err(ValidationError("can't add links to non-public posts").into());
    };

    // Reply validation
    if let Some(ref in_reply_to) = maybe_in_reply_to {
        if in_reply_to.repost_of_id.is_some() {
            return Err(ValidationError("can't reply to repost").into());
        };
        if in_reply_to.visibility != Visibility::Public &&
                visibility != Visibility::Direct {
            return Err(ValidationError("reply must have direct visibility").into());
        };
        if visibility != Visibility::Public {
            let mut in_reply_to_audience: Vec<_> = in_reply_to.mentions.iter()
                .map(|profile| profile.id).collect();
            in_reply_to_audience.push(in_reply_to.author.id);
            if !mentions.iter().all(|id| in_reply_to_audience.contains(id)) {
                return Err(ValidationError("audience can't be expanded").into());
            };
        };
    };

    // Create post
    let post_data = PostCreateData {
        content: content,
        content_source: maybe_content_source,
        in_reply_to_id: status_data.in_reply_to_id,
        repost_of_id: None,
        visibility: visibility,
        is_sensitive: status_data.sensitive,
        attachments: status_data.media_ids.unwrap_or(vec![]),
        mentions: mentions,
        tags: hashtags,
        links: links,
        emojis: emojis.iter().map(|emoji| emoji.id).collect(),
        object_id: None,
        created_at: Utc::now(),
    };
    validate_post_create_data(&post_data)?;
    validate_post_mentions(&post_data.mentions, &post_data.visibility)?;
    let mut post = create_post(db_client, &current_user.id, post_data).await?;
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
    ).await?.enqueue(db_client).await?;

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
    db_pool: web::Data<DbPool>,
    status_data: web::Json<StatusPreviewData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let instance = config.instance();
    let status_data = status_data.into_inner();
    let content = match status_data.content_type.as_str() {
        "text/html" => status_data.status,
        "text/markdown" => {
            markdown_lite_to_html(&status_data.status)
                .map_err(|_| ValidationError("invalid markdown"))?
        },
        _ => return Err(ValidationError("unsupported content type").into()),
    };
    let PostContent { mut content, emojis, .. } = parse_microsyntaxes(
        db_client,
        &instance,
        content,
    ).await?;
    // Clean content
    content = clean_local_content(&content)?;
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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let post = get_post_by_id(db_client, &status_id).await?;
    if !can_view_post(db_client, maybe_current_user.as_ref(), &post).await? {
        return Err(MastodonError::NotFoundError("post"));
    };
    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        maybe_current_user.as_ref(),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[delete("/{status_id}")]
async fn delete_status(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id(db_client, &status_id).await?;
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
    let deletion_queue = delete_post(db_client, &status_id).await?;
    tokio::spawn(async move {
        remove_media(&config, deletion_queue).await;
    });
    delete_note.enqueue(db_client).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[get("/{status_id}/context")]
async fn get_context(
    auth: Option<BearerAuth>,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let posts = get_thread(
        db_client,
        &status_id,
        maybe_current_user.as_ref().map(|user| &user.id),
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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let posts = get_thread(
        db_client,
        &status_id,
        maybe_current_user.as_ref().map(|user| &user.id),
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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if post.repost_of_id.is_some() {
        return Err(MastodonError::NotFoundError("post"));
    };
    let maybe_reaction_created = match create_reaction(
        db_client, &current_user.id, &status_id, None,
    ).await {
        Ok(reaction) => {
            post.reaction_count += 1;
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
            &reaction.id,
        ).await?.enqueue(db_client).await?;
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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    let maybe_reaction_deleted = match delete_reaction(
        db_client, &current_user.id, &status_id,
    ).await {
        Ok(reaction_id) => {
            post.reaction_count -= 1;
            Some(reaction_id)
        },
        Err(DatabaseError::NotFound(_)) => None, // post not favourited
        Err(other_error) => return Err(other_error.into()),
    };

    if let Some(reaction_id) = maybe_reaction_deleted {
        // Federate
        prepare_undo_like(
            db_client,
            &config.instance(),
            &current_user,
            &post,
            &reaction_id,
        ).await?.enqueue(db_client).await?;
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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !can_create_post(&current_user) {
        return Err(MastodonError::PermissionError);
    };
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if !post.is_public() || post.repost_of_id.is_some() {
        return Err(MastodonError::NotFoundError("post"));
    };
    let repost_data = PostCreateData::repost(status_id.into_inner(), None);
    let mut repost = create_post(db_client, &current_user.id, repost_data).await?;
    post.repost_count += 1;
    repost.repost_of = Some(Box::new(post));

    // Federate
    prepare_announce(
        db_client,
        &config.instance(),
        &current_user,
        &repost,
    ).await?.enqueue(db_client).await?;

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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let repost_id = get_repost_by_author(
        db_client,
        &status_id,
        &current_user.id,
    ).await?;
    // Ignore returned data because reposts don't have attached files
    delete_post(db_client, &repost_id).await?;
    let post = get_post_by_id(db_client, &status_id).await?;

    // Federate
    prepare_undo_announce(
        db_client,
        &config.instance(),
        &current_user,
        &post,
        &repost_id,
    ).await?.enqueue(db_client).await?;

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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if post.author.id != current_user.id || !post.is_public() || post.repost_of_id.is_some() {
        return Err(MastodonError::OperationError("can't pin post"));
    };
    set_pinned_flag(db_client, &post.id, true).await?;
    post.is_pinned = true;

    prepare_add_note(
        db_client,
        &config.instance(),
        &current_user,
        &post.id,
    ).await?.enqueue(db_client).await?;

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
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if post.author.id != current_user.id || !post.is_public() || post.repost_of_id.is_some() {
        return Err(MastodonError::OperationError("can't unpin post"));
    };
    set_pinned_flag(db_client, &post.id, false).await?;
    post.is_pinned = false;

    prepare_remove_note(
        db_client,
        &config.instance(),
        &current_user,
        &post.id,
    ).await?.enqueue(db_client).await?;

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[cfg(feature = "ethereum-extras")]
use {
    mitra_models::posts::queries::set_post_token_tx_id,
    mitra_services::{
        ethereum::{
            erc721_metadata::PostMetadata,
            nft::create_mint_signature,
        },
        ipfs::utils::get_ipfs_url,
    },
    mitra_utils::currencies::Currency,
    super::types::TransactionData,
};

#[cfg(feature = "ethereum-extras")]
fn create_post_metadata(
    post_id: &Uuid,
    post_url: &str,
    content: &str,
    created_at: &DateTime<Utc>,
    image_cid: Option<&str>,
) -> PostMetadata { PostMetadata::new(post_id, post_url, content, created_at, image_cid) }

#[cfg(not(feature = "ethereum-extras"))]
fn create_post_metadata(
    _: &Uuid,
    _: &str,
    _: &str,
    _: &DateTime<Utc>,
    _: Option<&str>,
) -> () { }

#[post("/{status_id}/make_permanent")]
async fn make_permanent(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if post.ipfs_cid.is_some() {
        return Err(MastodonError::OperationError("post already saved to IPFS"));
    };
    if post.author.id != current_user.id || !post.is_public() || post.repost_of_id.is_some() {
        // Users can only archive their own public posts
        return Err(MastodonError::PermissionError);
    };
    let ipfs_api_url = config.ipfs_api_url.as_ref()
        .ok_or(MastodonError::NotSupported)?;

    let mut attachments = vec![];
    for attachment in post.attachments.iter_mut() {
        // Add attachment to IPFS
        let image_data = read_file(&config.media_dir(), &attachment.file_name)
            .map_err(|_| MastodonError::InternalError)?;
        let image_cid = ipfs_store::add(ipfs_api_url, image_data).await
            .map_err(|_| MastodonError::InternalError)?;
        attachment.ipfs_cid = Some(image_cid.clone());
        attachments.push((attachment.id, image_cid));
    };
    assert!(post.is_local());
    let instance = config.instance();
    let post_metadata = if cfg!(feature = "ethereum-extras") {
        let post_url = local_object_id(&instance.url(), &post.id);
        let maybe_post_image_cid = post.attachments.first()
            .and_then(|attachment| attachment.ipfs_cid.as_deref());
        #[allow(clippy::let_unit_value)]
        let post_metadata = create_post_metadata(
            &post.id,
            &post_url,
            &post.content,
            &post.created_at,
            maybe_post_image_cid,
        );
        serde_json::to_value(post_metadata)
            .map_err(|_| MastodonError::InternalError)?
    } else {
        let note = build_note(
            &instance.hostname(),
            &instance.url(),
            &post,
            config.federation.fep_e232_enabled,
            true,
        );
        serde_json::to_value(note)
            .map_err(|_| MastodonError::InternalError)?
    };
    let post_metadata_json = post_metadata.to_string().as_bytes().to_vec();
    let post_metadata_cid = ipfs_store::add(ipfs_api_url, post_metadata_json).await
        .map_err(|_| MastodonError::InternalError)?;

    set_post_ipfs_cid(db_client, &post.id, &post_metadata_cid, attachments).await?;
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

#[cfg(feature = "ethereum-extras")]
#[get("/{status_id}/signature")]
async fn get_signature(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let ethereum_config = config.ethereum_config()
        .ok_or(MastodonError::NotSupported)?;
    // User must have a public ethereum address
    let wallet_address = current_user
        .public_wallet_address(&Currency::Ethereum)
        .ok_or(MastodonError::PermissionError)?;
    let post = get_post_by_id(db_client, &status_id).await?;
    if post.author.id != current_user.id || !post.is_public() || post.repost_of_id.is_some() {
        // Users can only tokenize their own public posts
        return Err(MastodonError::PermissionError);
    };
    let ipfs_cid = post.ipfs_cid
        // Post metadata is not immutable
        .ok_or(MastodonError::PermissionError)?;
    let token_uri = get_ipfs_url(&ipfs_cid);
    let signature = create_mint_signature(
        ethereum_config,
        &wallet_address,
        &token_uri,
    ).map_err(|_| MastodonError::InternalError)?;
    Ok(HttpResponse::Ok().json(signature))
}

#[cfg(feature = "ethereum-extras")]
#[post("/{status_id}/token_minted")]
async fn token_minted(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    status_id: web::Path<Uuid>,
    transaction_data: web::Json<TransactionData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if post.token_tx_id.is_some() {
        return Err(MastodonError::OperationError("transaction is already registered"));
    };
    if post.author.id != current_user.id || !post.is_public() || post.repost_of_id.is_some() {
        return Err(MastodonError::PermissionError);
    };
    let token_tx_id = transaction_data.into_inner().transaction_id;
    set_post_token_tx_id(db_client, &post.id, &token_tx_id).await?;
    post.token_tx_id = Some(token_tx_id);

    let status = build_status(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        Some(&current_user),
        post,
    ).await?;
    Ok(HttpResponse::Ok().json(status))
}

#[cfg(feature = "ethereum-extras")]
fn with_ethereum_extras(scope: Scope) -> Scope {
    scope
        .service(get_signature)
        .service(token_minted)
}
#[cfg(not(feature = "ethereum-extras"))]
fn with_ethereum_extras(scope: Scope) -> Scope {
    scope
}

pub fn status_api_scope() -> Scope {
    let scope = web::scope("/api/v1/statuses")
        // Routes without status ID
        .service(create_status)
        .service(preview_status)
        // Routes with status ID
        .service(get_status)
        .service(delete_status)
        .service(get_context)
        .service(get_thread_view)
        .service(favourite)
        .service(unfavourite)
        .service(reblog)
        .service(unreblog)
        .service(pin)
        .service(unpin)
        .service(make_permanent);
    with_ethereum_extras(scope)
}
