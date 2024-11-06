use std::path::Path;

use actix_files::{Files, NamedFile};
use actix_web::{
    dev::{fn_service, ServiceRequest, ServiceResponse},
    guard,
    web,
    HttpResponse,
    Resource,
};
use uuid::Uuid;

use apx_sdk::http_server::is_activitypub_request;
use mitra_activitypub::identifiers::{post_object_id, profile_actor_id};
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

use crate::errors::HttpError;

use super::urls::get_opengraph_image_url;

const INDEX_FILE: &str = "index.html";
const CUSTOM_CSS_PATH: &str = "/assets/custom.css";

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
        is_activitypub_request(ctx.head().headers())
    })
}

fn opengraph_guard(with_opengraph: bool) -> impl guard::Guard {
    guard::fn_guard(move |_ctx| {
        // TODO: use .app_data (actix-web 4.7.0)
        with_opengraph
    })
}

async fn profile_page_redirect_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    acct: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_acct(db_client, &acct).await?;
    let actor_id = profile_actor_id(&config.instance_url(), &profile);
    let response = HttpResponse::Found()
        .append_header(("Location", actor_id))
        .finish();
    Ok(response)
}

pub fn profile_page_redirect() -> Resource {
    web::resource("/@{acct}")
        .guard(activitypub_guard())
        .route(web::get().to(profile_page_redirect_view))
}

/// Redirect to ActivityPub representation
async fn post_page_redirect_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    post_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let post = get_post_by_id(db_client, *post_id).await?;
    let object_id = post_object_id(&config.instance_url(), &post);
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
        .ok_or(HttpError::InternalError)?;
    let index_html = std::fs::read_to_string(web_client_dir.join(INDEX_FILE))
        .map_err(|_| HttpError::InternalError)?;
    let page = match get_post_by_id(db_client, *post_id).await {
        Ok(post) if post.is_public() => {
            // Rewrite index.html and insert metadata
            let metadata_block = format!(
                include_str!("metadata_block.html"),
                instance_title=config.instance_title,
                title=format!("Post by @{}", post.author.preferred_handle()),
                image_url=get_opengraph_image_url(&config.instance_url()),
            );
            index_html.replace(
                "<title>Mitra - Federated social network</title>",
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

pub fn post_page_overlay(config: &Config) -> Resource {
    let with_opengraph = config.web_client_dir.is_some() && config.web_client_rewrite_index;
    web::resource("/post/{object_id}")
        .guard(guard::Any(activitypub_guard()).or(opengraph_guard(with_opengraph)))
        .route(web::get().guard(activitypub_guard()).to(post_page_redirect_view))
        .route(web::get().guard(opengraph_guard(with_opengraph)).to(post_page_opengraph_view))
}
