use actix_web::{
    dev::ConnectionInfo,
    get,
    web,
    HttpResponse,
    Scope,
};

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    emojis::queries::get_local_emojis,
};

use crate::http::get_request_base_url;
use crate::mastodon_api::{
    errors::MastodonError,
    media_server::ClientMediaServer,
};
use super::types::CustomEmoji;

/// https://docs.joinmastodon.org/methods/custom_emojis/
#[get("")]
async fn custom_emoji_list(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let base_url = get_request_base_url(connection_info);
    let media_server = ClientMediaServer::new(&config, &base_url);
    let emojis: Vec<CustomEmoji> = get_local_emojis(db_client).await?
        .into_iter()
        .map(|db_emoji| CustomEmoji::from_db(&media_server, db_emoji))
        .collect();
    Ok(HttpResponse::Ok().json(emojis))
}

pub fn custom_emoji_api_scope() -> Scope {
    web::scope("/v1/custom_emojis")
        .service(custom_emoji_list)
}
