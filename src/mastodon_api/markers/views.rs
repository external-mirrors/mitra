use actix_web::{get, post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    markers::queries::{
        create_or_update_marker,
        get_marker_opt,
    },
    markers::types::Timeline,
};
use mitra_validators::errors::ValidationError;

use crate::{
    http::MultiQuery,
    mastodon_api::{
        auth::get_current_user,
        errors::MastodonError,
    },
};

use super::types::{MarkerQueryParams, MarkerCreateData, Markers};

// https://docs.joinmastodon.org/methods/markers/#get
#[get("")]
async fn get_marker_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: MultiQuery<MarkerQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let timelines = query_params.to_timelines()?;
    let mut maybe_home_marker = None;
    let mut maybe_notifications_marker = None;
    for timeline in timelines {
        // Each marker type is processed only once
        if timeline == Timeline::Home && maybe_home_marker.is_none() {
            maybe_home_marker =
                get_marker_opt(db_client, current_user.id, timeline).await?;
        };
        if timeline == Timeline::Notifications && maybe_notifications_marker.is_none() {
            maybe_notifications_marker =
                get_marker_opt(db_client, current_user.id, timeline).await?;
        };
    };
    let markers = Markers {
        home: maybe_home_marker.map(|db_marker| db_marker.into()),
        notifications: maybe_notifications_marker.map(|db_marker| db_marker.into()),
    };
    Ok(HttpResponse::Ok().json(markers))
}

// https://docs.joinmastodon.org/methods/markers/#create
#[post("")]
async fn update_marker_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    marker_data: web::Json<MarkerCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let (timeline, last_read_id) = if let Some(ref last_read_id) = marker_data.home {
        (Timeline::Home, last_read_id)
    } else if let Some(ref last_read_id) = marker_data.notifications {
        (Timeline::Notifications, last_read_id)
    } else {
        return Err(ValidationError("marker data is missing").into());
    };
    let db_marker = create_or_update_marker(
        db_client,
        current_user.id,
        timeline,
        last_read_id,
    ).await?;
    let (maybe_home_marker, maybe_notifications_marker) = match db_marker.timeline {
        Timeline::Home => (Some(db_marker.into()), None),
        Timeline::Notifications => (None, Some(db_marker.into())),
    };
    let markers = Markers {
        home: maybe_home_marker,
        notifications: maybe_notifications_marker,
    };
    Ok(HttpResponse::Ok().json(markers))
}

pub fn marker_api_scope() -> Scope {
    web::scope("/v1/markers")
        .service(get_marker_view)
        .service(update_marker_view)
}
