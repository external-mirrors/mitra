use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::markers::types::{DbTimelineMarker, Timeline};
use mitra_validators::errors::ValidationError;

use crate::mastodon_api::serializers::serialize_datetime;

#[derive(Deserialize)]
pub struct MarkerQueryParams {
    pub timeline: Vec<String>,
}

impl MarkerQueryParams {
    pub fn to_timelines(&self) -> Result<Vec<Timeline>, ValidationError> {
        let mut timelines = vec![];
        for value in &self.timeline {
            let timeline = match value.as_str() {
                "home" => Timeline::Home,
                "notifications" => Timeline::Notifications,
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
