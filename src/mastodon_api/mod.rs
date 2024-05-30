use actix_web::{web, Scope};

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
pub mod settings;
mod statuses;
mod subscriptions;
mod timelines;

mod auth;
mod deserializers;
mod errors;
mod pagination;
mod uploads;

const MASTODON_API_VERSION: &str = "4.0.0";

pub use oauth::views::oauth_api_scope;

pub fn mastodon_api_scope() -> Scope {
    web::scope("/api")
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
