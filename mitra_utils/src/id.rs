use std::time::SystemTime;
use chrono::{DateTime, Utc};
use ulid::Ulid;
use uuid::Uuid;

/// Produces new lexicographically sortable ID
pub fn generate_ulid() -> Uuid {
    let ulid = Ulid::new();
    Uuid::from(ulid)
}

pub fn datetime_to_ulid(datetime: DateTime<Utc>) -> Uuid {
    let system_time = SystemTime::from(datetime);
    let ulid = Ulid::from_datetime(system_time);
    Uuid::from(ulid)
}
