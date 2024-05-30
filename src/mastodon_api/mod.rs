use actix_web::{
    body::{BodySize, BoxBody, EitherBody, MessageBody},
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    http::StatusCode,
    middleware::{
        ErrorHandlerResponse,
        ErrorHandlers,
    },
    web,
    Error,
    Scope,
};

mod accounts;
mod apps;
mod custom_emojis;
mod directory;
mod follow_requests;
mod instance;
mod markers;
mod media;
mod notifications;
mod oauth;
mod search;
mod settings;
mod statuses;
mod subscriptions;
mod timelines;

mod auth;
mod deserializers;
mod errors;
mod pagination;
mod uploads;

const MASTODON_API_VERSION: &str = "4.0.0";

use errors::MastodonErrorData;
pub use oauth::views::oauth_api_scope;

/// Error handler for 401 Unauthorized
fn create_auth_error_handler() -> ErrorHandlers<BoxBody> {
    // Creates and returns actix middleware
    ErrorHandlers::new()
        .handler(StatusCode::UNAUTHORIZED, move |response| {
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

pub fn mastodon_api_scope() -> Scope<impl ServiceFactory<
    ServiceRequest,
    Config = (),
    Response = ServiceResponse<EitherBody<BoxBody>>,
    Error = Error,
    InitError = (),
>> {
    web::scope("/api")
        .wrap(create_auth_error_handler())
        .service(accounts::views::account_api_scope())
        .service(apps::views::application_api_scope())
        .service(custom_emojis::views::custom_emoji_api_scope())
        .service(directory::views::directory_api_scope())
        .service(follow_requests::views::follow_request_api_scope())
        .service(instance::views::instance_api_v1_scope())
        .service(instance::views::instance_api_v2_scope())
        .service(markers::views::marker_api_scope())
        .service(media::views::media_api_v1_scope())
        .service(media::views::media_api_v2_scope())
        .service(notifications::views::notification_api_scope())
        .service(search::views::search_api_scope())
        .service(settings::views::settings_api_scope())
        .service(statuses::views::status_api_scope())
        .service(subscriptions::views::subscription_api_scope())
        .service(timelines::views::timeline_api_scope())
}
