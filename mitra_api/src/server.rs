use std::str::FromStr;
use std::net::SocketAddrV4;
use std::path::Path;

use actix_cors::{Cors, CorsError};
use actix_web::{
    dev::Service,
    http::{
        header as http_header,
        Method,
    },
    middleware::{
        ErrorHandlers,
        ErrorHandlerResponse,
        Logger as ActixLogger,
        NormalizePath,
    },
    web,
    App,
    HttpResponse,
    HttpServer,
};
use apx_core::http_url_whatwg::get_hostname;
use log::Level;

use mitra_config::{Config, Environment};
use mitra_models::database::DatabaseConnectionPool;
use mitra_services::{
    media::{FilesystemServer, MediaStorage},
};
use mitra_utils::files::set_file_permissions;

use crate::activitypub::views as activitypub;
use crate::atom::views::atom_scope;
use crate::http::{
    create_default_headers_middleware,
    json_error_handler,
    log_response_error,
};
use crate::mastodon_api::{mastodon_api_scope, oauth_api_scope};
use crate::metrics::views::metrics_api_scope;
use crate::nodeinfo::views as nodeinfo;
use crate::state::AppState;
use crate::webfinger::views as webfinger;
use crate::web_client::views as web_client;

pub async fn run_server(
    config: Config,
    db_pool: DatabaseConnectionPool,
) -> std::io::Result<()> {
    let app_state = web::Data::new(AppState::default());
    let media_storage = MediaStorage::new(&config);
    let num_workers = std::cmp::max(num_cpus::get(), 4);
    let http_socket_addr = config.http_socket();
    let http_socket_perms = config.http_socket_perms;
    if config.media_proxy_enabled {
        log::info!("media proxy enabled");
    };

    let http_server = HttpServer::new(move || {
        let cors_config = match config.environment {
            Environment::Development => {
                Cors::permissive()
            },
            Environment::Production => {
                // Mastodon: https://github.com/mastodon/mastodon/blob/v4.4.5/config/initializers/cors.rb
                let mut cors_config = Cors::default();
                // TODO: allow all origins if `http_cors_allowlist` is not set
                if !config.http_cors_allow_all {
                    // Strict mode
                    let allowlist = config.http_cors_allowlist
                        .clone()
                        .unwrap_or_default();
                    for origin in allowlist {
                        cors_config = cors_config.allowed_origin(&origin);
                    };
                    cors_config = cors_config
                        .allowed_origin(config.instance().uri_str())
                        // TODO: don't accept GET requests from disallowed origins
                        // TODO: don't automatically allow localhost in strict mode
                        .allowed_origin_fn(|origin, req_head| {
                            if req_head.method == Method::GET {
                                // Allow all GET requests
                                return true;
                            };
                            let maybe_hostname = origin.to_str().ok()
                                .and_then(|origin| get_hostname(origin).ok());
                            match maybe_hostname {
                                Some(hostname) => {
                                    hostname == "localhost" ||
                                    hostname == "127.0.0.1"
                                },
                                None => false,
                            }
                        });
                } else {
                    cors_config = cors_config.allow_any_origin();
                };
                cors_config
                    .allow_any_method()
                    .allow_any_header()
                    .expose_any_header()
            },
        };
        let payload_size_limit = 2 * config.limits.media.file_size_limit;
        let mut app = App::new()
            // NOTE: middlewares are executed in the reverse order
            // https://docs.rs/actix-web/latest/actix_web/middleware/#ordering
            .wrap(NormalizePath::trim())
            .wrap(cors_config)
            .wrap(ErrorHandlers::new()
                .default_handler_client(|response| {
                    if let Some(error) = response.response().error() {
                        if error.as_error::<CorsError>().is_some() {
                            log_response_error(Level::Warn, &response);
                        };
                    };
                    Ok(ErrorHandlerResponse::Response(response.map_into_left_body()))
                })
                .default_handler_server(|response| {
                   log_response_error(Level::Error, &response);
                   Ok(ErrorHandlerResponse::Response(response.map_into_left_body()))
                })
            )
            .wrap_fn(|request, service| {
                // Fix for https://github.com/actix/actix-web/issues/3191
                let path = request.path().to_owned();
                let fut = service.call(request);
                async move {
                    let mut response = fut.await?;
                    if path.starts_with(FilesystemServer::BASE_PATH) {
                        response.headers_mut()
                            .remove(http_header::CONTENT_ENCODING);
                    };
                    Ok(response)
                }
            })
            .wrap(create_default_headers_middleware())
            .wrap(ActixLogger::new("%r : %s : %{r}a"))
            .app_data(web::PayloadConfig::default().limit(payload_size_limit))
            .app_data(web::JsonConfig::default()
                .limit(payload_size_limit)
                .error_handler(json_error_handler)
            )
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::clone(&app_state))
            .service(oauth_api_scope())
            .service(mastodon_api_scope(payload_size_limit))
            .service(metrics_api_scope(config.metrics.is_some()))
            .service(webfinger::webfinger_view)
            .service(activitypub::actor_scope())
            .service(activitypub::instance_actor_scope())
            .service(activitypub::object_view)
            .service(activitypub::replies_collection)
            .service(activitypub::emoji_view)
            .service(activitypub::tag_view)
            .service(activitypub::conversation_view)
            .service(activitypub::activity_view)
            .service(activitypub::gateway_scope(config.federation.fep_ef61_gateway_enabled))
            .service(activitypub::media_gateway_scope(config.federation.fep_ef61_gateway_enabled))
            .service(atom_scope())
            .service(nodeinfo::get_nodeinfo_jrd)
            .service(nodeinfo::get_nodeinfo_2_0)
            .service(nodeinfo::get_nodeinfo_2_1)
            .service(web_client::profile_page_overlay())
            .service(web_client::post_page_overlay())
            .service(
                // Fallback for well-known paths
                web::resource("/.well-known/{path}")
                    .to(HttpResponse::NotFound)
            );
        #[allow(irrefutable_let_patterns)]
        if let MediaStorage::Filesystem(ref backend) = media_storage {
            app = app.service(actix_files::Files::new(
                FilesystemServer::BASE_PATH,
                backend.media_dir.clone(),
            ));
        };
        if let Some(ref web_client_dir) = config.web_client_dir {
            app = app.service(web_client::themeable_web_client_service(
                web_client_dir,
                config.web_client_theme_dir.as_deref(),
            ));
        };
        app
    });

    let http_server = if let Ok(addr) = SocketAddrV4::from_str(&http_socket_addr) {
        http_server.bind(addr)?
    } else {
        // Assume unix socket path
        let http_socket_path = Path::new(&http_socket_addr);
        let http_server = http_server.bind_uds(http_socket_path)?;
        if let Some(socket_perms) = http_socket_perms {
            set_file_permissions(http_socket_path, socket_perms)?;
        };
        http_server
    };
    log::info!("listening on {}", http_socket_addr);
    http_server
        .workers(num_workers)
        .run()
        .await?;
    log::info!("server terminated");
    Ok(())
}
