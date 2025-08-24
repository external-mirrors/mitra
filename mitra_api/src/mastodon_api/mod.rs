use actix_multipart::form::MultipartFormConfig;
use actix_web::{
    body::{BodySize, BoxBody, EitherBody, MessageBody},
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    error::ResponseError,
    http::StatusCode,
    middleware::{
        ErrorHandlerResponse,
        ErrorHandlers,
    },
    web,
    Error,
    HttpRequest,
    Scope,
};
use log::Level;
use serde_qs::{
    actix::QsQueryConfig,
    Config as QsConfig,
};

use crate::http::log_response_error;

mod accounts;
mod admin;
mod apps;
mod bookmarks;
mod custom_emojis;
mod directory;
mod filters;
mod follow_requests;
mod instance;
mod lists;
mod markers;
mod media;
mod media_proxy;
mod mutes;
mod notifications;
mod oauth;
mod polls;
mod reactions;
mod search;
mod settings;
mod statuses;
mod subscriptions;
mod timelines;

mod auth;
mod errors;
mod media_server;
mod microsyntax;
mod pagination;
mod serializers;
mod uploads;

const MASTODON_API_VERSION: &str = "4.0.0";

use errors::{MastodonError, MastodonErrorData};
pub use oauth::views::oauth_api_scope;

fn validation_error_handler(
    error: impl ResponseError,
    _: &HttpRequest,
) -> Error {
    MastodonError::ValidationError(error.to_string()).into()
}

/// Error handler for 401 Unauthorized
fn create_error_handlers() -> ErrorHandlers<BoxBody> {
    // Creates and returns actix middleware
    ErrorHandlers::new()
        .default_handler_client(|response| {
            log_response_error(Level::Info, &response);
            Ok(ErrorHandlerResponse::Response(response.map_into_left_body()))
        })
        .handler(StatusCode::UNAUTHORIZED, |response| {
            let response_new = response.map_body(|_, body: BoxBody| {
                if let BodySize::None | BodySize::Sized(0) = body.size() {
                    // Insert error description if response body is empty
                    // https://github.com/actix/actix-extras/issues/156
                    let error_data =
                        MastodonErrorData::new("auth header is not present");
                    let error_data = serde_json::to_string(&error_data)
                        .expect("object should be serializable");
                    BoxBody::new(error_data)
                } else {
                    body.boxed()
                }
            });
            Ok(ErrorHandlerResponse::Response(response_new.map_into_right_body()))
        })
}

pub fn mastodon_api_scope(
    payload_size_limit: usize,
) -> Scope<impl ServiceFactory<
    ServiceRequest,
    Config = (),
    Response = ServiceResponse<EitherBody<BoxBody>>,
    Error = Error,
    InitError = (),
>> {
    let path_config = web::PathConfig::default()
        .error_handler(validation_error_handler);
    let query_config = web::QueryConfig::default()
        .error_handler(validation_error_handler);
    let json_config = web::JsonConfig::default()
        .limit(payload_size_limit)
        .error_handler(validation_error_handler);
    let form_config = web::FormConfig::default()
        .error_handler(validation_error_handler);
    let multipart_form_config = MultipartFormConfig::default()
        .total_limit(payload_size_limit)
        .memory_limit(payload_size_limit)
        .error_handler(validation_error_handler);
    // Disable strict mode
    let qs_config = QsConfig::new(2, false);
    let multiquery_config = QsQueryConfig::default()
        .qs_config(qs_config)
        .error_handler(validation_error_handler);
    web::scope("/api")
        .app_data(path_config)
        .app_data(query_config)
        .app_data(json_config)
        .app_data(form_config)
        .app_data(multipart_form_config)
        .app_data(multiquery_config)
        .wrap(create_error_handlers())
        .service(accounts::views::account_api_scope())
        .service(admin::posts::views::admin_post_api_scope())
        .service(apps::views::application_api_scope())
        .service(bookmarks::views::bookmark_api_scope())
        .service(custom_emojis::views::custom_emoji_api_scope())
        .service(directory::views::directory_api_scope())
        .service(filters::views::filter_api_scope())
        .service(follow_requests::views::follow_request_api_scope())
        .service(instance::views::instance_api_v1_scope())
        .service(instance::views::instance_api_v2_scope())
        .service(lists::views::list_api_scope())
        .service(markers::views::marker_api_scope())
        .service(media::views::media_api_v1_scope())
        .service(media::views::media_api_v2_scope())
        .service(media_proxy::views::media_proxy_scope())
        .service(mutes::views::mute_api_scope())
        .service(notifications::views::notification_api_v1_scope())
        .service(notifications::views::notification_api_v2_scope())
        .service(polls::views::poll_api_scope())
        .service(reactions::views::reaction_api_scope())
        .service(search::views::search_api_scope())
        .service(settings::views::settings_api_scope())
        .service(statuses::views::status_api_scope())
        .service(subscriptions::views::subscription_api_scope())
        .service(timelines::views::timeline_api_scope())
}
