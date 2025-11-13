use chrono::{DateTime, TimeDelta, Utc};

pub fn days_before_now(days: u32) -> DateTime<Utc> {
    Utc::now() - TimeDelta::days(days.into())
}
