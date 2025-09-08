use std::path::Path;

use actix_files::{Files, NamedFile};
use actix_web::{
    dev::{fn_service, ServiceRequest, ServiceResponse},
    guard,
    web,
    HttpResponse,
    Resource,
};
use apx_sdk::{
    core::http_types::header_map_adapter,
    http_server::is_activitypub_request,
};
use uuid::Uuid;

use mitra_activitypub::{
    identifiers::{
        compatible_post_object_id,
        compatible_profile_actor_id,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    posts::{
        queries::get_post_by_id,
    },
    profiles::queries::get_profile_by_acct,
};
use mitra_utils::html::extract_title;

use crate::{
    atom::urls::get_user_feed_url,
    errors::HttpError,
    templates::render_template,
};

use super::{
    types::MetadataBlock,
    urls::get_opengraph_image_url,
};

const INDEX_FILE: &str = "index.html";
const CUSTOM_CSS_PATH: &str = "/assets/custom.css";

const INDEX_TITLE_ELEMENT: &str = "<title>Mitra - Federated social network</title>";
// https://ogp.me/#types
const OG_TYPE_ARTICLE: &str = "article";
const OG_TYPE_PROFILE: &str = "profile";

fn web_client_service(web_client_dir: &Path) -> Files {
    Files::new("/", web_client_dir)
        .index_file(INDEX_FILE)
        .prefer_utf8(true)
        .use_hidden_files()
        .default_handler(fn_service(|service_request: ServiceRequest| {
            // Workaround for https://github.com/actix/actix-web/issues/2617
            let (request, _) = service_request.into_parts();
            let index_path = request.app_data::<web::Data<Config>>()
                .expect("app data should contain config")
                .web_client_dir.as_ref()
                .expect("web_client_dir should be present in config")
                .join(INDEX_FILE);
            async {
                if request.path() == CUSTOM_CSS_PATH {
                    // Don't serve index.html if custom.css doesn't exist
                    let response = HttpResponse::NotFound().finish();
                    return Ok(ServiceResponse::new(request, response));
                };
                let index_file = NamedFile::open_async(index_path).await?;
                let response = index_file.into_response(&request);
                Ok(ServiceResponse::new(request, response))
            }
        }))
}

pub fn themeable_web_client_service(
    web_client_dir: &Path,
    maybe_theme_dir: Option<&Path>,
) -> Files {
    let service = web_client_service(web_client_dir);
    if let Some(theme_dir) = maybe_theme_dir {
        Files::new("/", theme_dir)
            .index_file(INDEX_FILE)
            .default_handler(service)
    } else {
        service
    }
}

fn activitypub_guard() -> impl guard::Guard {
    guard::fn_guard(|ctx| {
        is_activitypub_request(&header_map_adapter(ctx.head().headers()))
    })
}

fn opengraph_guard() -> impl guard::Guard {
    guard::fn_guard(|ctx| {
        let config = ctx.app_data::<web::Data<Config>>()
            .expect("app data should contain config");
        config.web_client_dir.is_some() && config.web_client_rewrite_index
    })
}

async fn profile_page_redirect_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    acct: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_acct(db_client, &acct).await?;
    let actor_id = compatible_profile_actor_id(&config.instance_url(), &profile);
    let response = HttpResponse::Found()
        .append_header(("Location", actor_id))
        .finish();
    Ok(response)
}

async fn profile_page_opengraph_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    acct: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let web_client_dir = config.web_client_dir.as_ref()
        .expect("web_client_dir should be defined");
    let index_html = std::fs::read_to_string(web_client_dir.join(INDEX_FILE))
        .map_err(HttpError::from_internal)?;
    let page = match get_profile_by_acct(db_client, &acct).await {
        Ok(profile) => {
            // Rewrite index.html and insert metadata
            let title = format!("Profile - {}", profile.preferred_handle());
            let maybe_atom_url = if profile.is_local() {
                let atom_url = get_user_feed_url(
                    &config.instance_url(),
                    &profile.username,
                );
                Some(atom_url)
            } else {
                None
            };
            let context = MetadataBlock {
                title: title.clone(),
                title_short: title,
                instance_title: config.instance_title.clone(),
                page_type: OG_TYPE_PROFILE,
                image_url: get_opengraph_image_url(&config.instance_url()),
                atom_url: maybe_atom_url,
            };
            let metadata_block = render_template(
                include_str!("templates/metadata_block.html"),
                context,
            )?;
            index_html.replace(
                INDEX_TITLE_ELEMENT,
                &metadata_block,
            )
        },
        Err(DatabaseError::NotFound(_)) => {
            // Don't insert metadata if profile doesn't exist or not public
            index_html
        },
        Err(other_error) => return Err(other_error.into()),
    };
    let response = HttpResponse::Ok()
        .content_type("text/html")
        .body(page);
    Ok(response)
}

pub fn profile_page_overlay() -> Resource {
    web::resource("/@{acct}")
        .guard(guard::Any(activitypub_guard()).or(opengraph_guard()))
        .route(web::get().guard(activitypub_guard()).to(profile_page_redirect_view))
        .route(web::get().guard(opengraph_guard()).to(profile_page_opengraph_view))
}

/// Redirect to ActivityPub representation
async fn post_page_redirect_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    post_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let post = get_post_by_id(db_client, *post_id).await?;
    let object_id = compatible_post_object_id(&config.instance_url(), &post);
    let response = HttpResponse::Found()
        .append_header(("Location", object_id))
        .finish();
    Ok(response)
}

async fn post_page_opengraph_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    post_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let web_client_dir = config.web_client_dir.as_ref()
        .expect("web_client_dir should be defined");
    let index_html = std::fs::read_to_string(web_client_dir.join(INDEX_FILE))
        .map_err(HttpError::from_internal)?;
    let page = match get_post_by_id(db_client, *post_id).await {
        Ok(post) if post.is_public() => {
            // Rewrite index.html and insert metadata
            let title_short = format!("Post by @{}", post.author.preferred_handle());
            let title = if post.in_reply_to_id.is_none() {
                const TITLE_LENGTH_MAX: usize = 75;
                let title = extract_title(&post.content, TITLE_LENGTH_MAX);
                format!("{title} - {title_short}")
            } else {
                // Do not extract title
                title_short.clone()
            };
            let context = MetadataBlock {
                title: title,
                title_short: title_short,
                instance_title: config.instance_title.clone(),
                page_type: OG_TYPE_ARTICLE,
                image_url: get_opengraph_image_url(&config.instance_url()),
                atom_url: None,
            };
            let metadata_block = render_template(
                include_str!("templates/metadata_block.html"),
                context,
            )?;
            index_html.replace(
                INDEX_TITLE_ELEMENT,
                &metadata_block,
            )
        },
        Ok(_) | Err(DatabaseError::NotFound(_)) => {
            // Don't insert metadata if post doesn't exist or not public
            index_html
        },
        Err(other_error) => return Err(other_error.into()),
    };
    let response = HttpResponse::Ok()
        .content_type("text/html")
        .body(page);
    Ok(response)
}

pub fn post_page_overlay() -> Resource {
    web::resource("/post/{object_id}")
        .guard(guard::Any(activitypub_guard()).or(opengraph_guard()))
        .route(web::get().guard(activitypub_guard()).to(post_page_redirect_view))
        .route(web::get().guard(opengraph_guard()).to(post_page_opengraph_view))
}
