use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_models::markers::types::{DbTimelineMarker, Timeline};
use mitra_validators::errors::ValidationError;

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
pub struct MarkerCreateData {
    #[serde(rename = "notifications[last_read_id]")]
    pub notifications: String,
}

/// https://docs.joinmastodon.org/entities/marker/
#[derive(Serialize)]
pub struct Marker {
    last_read_id: String,
    version: i32,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Markers {
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
