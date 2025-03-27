// https://prometheus.io/docs/specs/om/open_metrics_spec/
use actix_web::{
    get,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::basic::BasicAuth;

use mitra_config::Config;
use mitra_models::{
    background_jobs::{
        queries::get_job_count,
        types::JobType,
    },
    database::{get_database_client, DatabaseConnectionPool},
};

use crate::errors::HttpError;

const OPENMETRICS_MEDIA_TYPE: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

#[get("")]
async fn metrics_view(
    auth: BasicAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, HttpError> {
    let Some(metrics_config) = config.metrics.as_ref() else {
        return Err(HttpError::PermissionError);
    };
    let is_valid =
        auth.user_id() == metrics_config.auth_username &&
        auth.password() == Some(&metrics_config.auth_password);
    if !is_valid {
        return Err(HttpError::AuthError("incorrect username or password"));
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let incoming_activities =
        get_job_count(db_client, JobType::IncomingActivity).await?;
    let outgoing_activities =
        get_job_count(db_client, JobType::OutgoingActivity).await?;
    let body = format!(
        include_str!("openmetrics.txt"),
        incoming_activity_queue_size=incoming_activities,
        outgoing_activity_queue_size=outgoing_activities,
    );
    let response = HttpResponse::Ok()
        .content_type(OPENMETRICS_MEDIA_TYPE)
        .body(body);
    Ok(response)
}

pub fn metrics_api_scope(metrics_enabled: bool) -> Scope {
    // Returns 404 if metrics are disabled
    let mut scope = web::scope("/metrics");
    if metrics_enabled {
        scope = scope.service(metrics_view);
    };
    scope
}
