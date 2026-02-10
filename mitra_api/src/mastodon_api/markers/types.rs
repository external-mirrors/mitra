use actix_multipart::form::{
    text::Text,
    MultipartForm,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::markers::types::{
    Timeline as DbTimeline,
    TimelineMarker as DbTimelineMarker,
};
use mitra_validators::errors::ValidationError;

use crate::mastodon_api::serializers::serialize_datetime;

#[derive(Deserialize)]
pub struct MarkerQueryParams {
    pub timeline: Vec<String>,
}

impl MarkerQueryParams {
    pub fn to_timelines(&self) -> Result<Vec<DbTimeline>, ValidationError> {
        let mut timelines = vec![];
        for value in &self.timeline {
            let timeline = match value.as_str() {
                "home" => DbTimeline::Home,
                "notifications" => DbTimeline::Notifications,
                _ => return Err(ValidationError("invalid timeline name")),
            };
            timelines.push(timeline);
        };
        Ok(timelines)
    }
}

#[derive(Deserialize)]
pub struct MarkerData {
    last_read_id: String,
}

#[derive(Deserialize)]
pub struct MarkerCreateData {
    // JSON
    // https://docs.joinmastodon.org/client/intro/#hash
    home: Option<MarkerData>,
    notifications: Option<MarkerData>,

    // Form data
    #[serde(rename = "home[last_read_id]")]
    pub home_last_read_id: Option<String>,
    #[serde(rename = "notifications[last_read_id]")]
    pub notifications_last_read_id: Option<String>,
}

impl MarkerCreateData {
    pub fn home_last_read_id(&self) -> Option<&String> {
        self.home.as_ref()
            .map(|marker| &marker.last_read_id)
            .or(self.home_last_read_id.as_ref())
    }

    pub fn notifications_last_read_id(&self) -> Option<&String> {
        self.notifications.as_ref()
            .map(|marker| &marker.last_read_id)
            .or(self.notifications_last_read_id.as_ref())
    }
}

#[derive(MultipartForm)]
pub struct MarkerCreateMultipartForm {
    #[multipart(rename = "home[last_read_id]")]
    home_last_read_id: Option<Text<String>>,
    #[multipart(rename = "notifications[last_read_id]")]
    notifications_last_read_id: Option<Text<String>>,
}

impl From<MarkerCreateMultipartForm> for MarkerCreateData {
    fn from(form: MarkerCreateMultipartForm) -> Self {
        Self {
            home: form.home_last_read_id.map(|value| {
                MarkerData { last_read_id: value.into_inner() }
            }),
            notifications: form.notifications_last_read_id.map(|value| {
                 MarkerData { last_read_id: value.into_inner() }
            }),
            home_last_read_id: None,
            notifications_last_read_id: None,
        }
    }
}

/// https://docs.joinmastodon.org/entities/marker/
#[derive(Serialize)]
pub struct Marker {
    last_read_id: String,
    version: i32,
    #[serde(serialize_with = "serialize_datetime")]
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Markers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<Marker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications: Option<Marker>,
}

impl From<DbTimelineMarker> for Marker {

    fn from(value: DbTimelineMarker) -> Self {
        Self {
            last_read_id: value.last_read_id,
            version: 0,
            updated_at: value.updated_at,
        }
    }
}
