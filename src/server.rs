use std::str::FromStr;
use std::net::SocketAddrV4;
use std::path::Path;

use actix_cors::Cors;
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
use log::Level;

use apx_core::urls::get_hostname;
use mitra_config::{Config, Environment};
use mitra_models::database::DatabaseConnectionPool;
use mitra_services::{
    media::{MediaStorage, MEDIA_ROOT_URL},
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

    let http_server = HttpServer::new(move || {
        let cors_config = match config.environment {
            Environment::Development => {
                Cors::permissive()
            },
            Environment::Production => {
                let mut cors_config = Cors::default();
                for origin in config.http_cors_allowlist.iter() {
                    cors_config = cors_config.allowed_origin(origin);
                };
                cors_config
                    .allowed_origin(&config.instance_url())
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
                    })
                    .allow_any_method()
                    .allow_any_header()
                    .expose_any_header()
            },
        };
        let payload_size_limit = 2 * config.limits.media.file_size_limit;
        let mut app = App::new()
            .wrap(NormalizePath::trim())
            .wrap(cors_config)
            .wrap(ActixLogger::new("%r : %s : %{r}a"))
            .wrap(ErrorHandlers::new()
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
                    if path.starts_with(MEDIA_ROOT_URL) {
                        response.headers_mut()
                            .remove(http_header::CONTENT_ENCODING);
                    };
                    Ok(response)
                }
            })
            .wrap(create_default_headers_middleware())
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
            .service(webfinger::webfinger_view)
            .service(activitypub::actor_scope())
            .service(activitypub::instance_actor_scope())
            .service(activitypub::object_view)
            .service(activitypub::replies_collection)
            .service(activitypub::emoji_view)
            .service(activitypub::tag_view)
            .service(activitypub::conversation_view)
            .service(activitypub::activity_view)
            .service(activitypub::gateway_scope())
            .service(atom_scope())
            .service(nodeinfo::get_nodeinfo_jrd)
            .service(nodeinfo::get_nodeinfo_2_0)
            .service(nodeinfo::get_nodeinfo_2_1)
            .service(web_client::profile_page_overlay(&config))
            .service(web_client::post_page_overlay(&config))
            .service(
                // Fallback for well-known paths
                web::resource("/.well-known/{path}")
                    .to(HttpResponse::NotFound)
            );
        #[allow(irrefutable_let_patterns)]
        if let MediaStorage::Filesystem(ref backend) = media_storage {
            app = app.service(actix_files::Files::new(
                MEDIA_ROOT_URL,
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
