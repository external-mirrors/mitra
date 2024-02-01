use std::path::Path;

use actix_files::{Files, NamedFile};
use actix_web::{
    dev::{fn_service, ServiceRequest, ServiceResponse},
    guard,
    http::header as http_header,
    web,
    web::Data,
    HttpResponse,
    Resource,
};
use uuid::Uuid;

use mitra_config::Config;
use mitra_federation::http_server::is_activitypub_request;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    posts::queries::get_post_by_id,
    profiles::queries::get_profile_by_acct,
};

use crate::activitypub::{
    identifiers::{post_object_id, profile_actor_id},
};
use crate::errors::HttpError;

pub fn static_service(web_client_dir: &Path) -> Files {
    Files::new("/", web_client_dir)
        .index_file("index.html")
        .prefer_utf8(true)
        .use_hidden_files()
        .default_handler(fn_service(|service_request: ServiceRequest| {
            // Workaround for https://github.com/actix/actix-web/issues/2617
            let (request, _) = service_request.into_parts();
            let index_path = request.app_data::<Data<Config>>()
                .expect("app data should contain config")
                .web_client_dir.as_ref()
                .expect("web_client_dir should be present in config")
                .join("index.html");
            async {
                let index_file = NamedFile::open_async(index_path).await?;
                let response = index_file.into_response(&request);
                Ok(ServiceResponse::new(request, response))
            }
        }))
}

fn activitypub_guard() -> impl guard::Guard {
    guard::fn_guard(|ctx| {
        is_activitypub_request(ctx.head().headers())
    })
}

fn opengraph_guard() -> impl guard::Guard {
    guard::fn_guard(|ctx| {
        let headers = ctx.head().headers();
        let maybe_user_agent = headers.get(http_header::USER_AGENT)
            .and_then(|value| value.to_str().ok());
        if let Some(user_agent) = maybe_user_agent {
            user_agent == "Synapse (bot; +https://github.com/matrix-org/synapse)"
        } else { false }
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

async fn post_page_redirect_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    post_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let post = get_post_by_id(db_client, &post_id).await?;
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
    let post = get_post_by_id(db_client, &post_id).await?;
    let page = format!(
        include_str!("opengraph.html"),
        acct=post.author.acct,
        instance_url=config.instance_url(),
        image_path="/ogp-image.png",
    );
    let response = HttpResponse::Ok()
        .content_type("text/html")
        .body(page);
    Ok(response)
}

pub fn post_page_redirect() -> Resource {
    web::resource("/post/{object_id}")
        .guard(guard::Any(activitypub_guard()).or(opengraph_guard()))
        .route(web::get().guard(activitypub_guard()).to(post_page_redirect_view))
        .route(web::get().guard(opengraph_guard()).to(post_page_opengraph_view))
}
