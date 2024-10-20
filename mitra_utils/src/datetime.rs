use chrono::{DateTime, Duration, Utc};

pub fn days_before_now(days: u32) -> DateTime<Utc> {
    Utc::now() - Duration::days(days.into())
}
