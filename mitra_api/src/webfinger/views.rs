// https://webfinger.net/
use actix_web::{get, web, HttpResponse};
use apx_sdk::{
    jrd::JRD_MEDIA_TYPE,
};
use serde::Deserialize;

use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
    },
};

use crate::errors::HttpError;

use super::helpers::get_jrd;

#[derive(Deserialize)]
struct WebfingerQueryParams {
    resource: String,
}

#[get("/.well-known/webfinger")]
pub async fn webfinger_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<WebfingerQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let jrd = get_jrd(
        db_client,
        config.instance(),
        query_params.resource.as_str(),
    ).await?;
    let response = HttpResponse::Ok()
        .content_type(JRD_MEDIA_TYPE)
        .json(jrd);
    Ok(response)
}
