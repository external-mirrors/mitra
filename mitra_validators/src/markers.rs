use uuid::Uuid;

use mitra_models::markers::types::Timeline;

use super::errors::ValidationError;

pub fn validate_marker_data(
    timeline: Timeline,
    last_read_id: &str,
) -> Result<(), ValidationError> {
    match timeline {
        Timeline::Home => {
            last_read_id.parse::<Uuid>()
                .map_err(|_| ValidationError("invalid item ID"))?;
        },
        Timeline::Notifications => {
            last_read_id.parse::<i32>()
                .map_err(|_| ValidationError("invalid item ID"))?;
        },
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use mitra_models::{
        notifications::types::NotificationDetailed,
        posts::types::PostDetailed,
    };
    use super::*;

    #[test]
    fn test_validate_marker_data_home() {
        let post = PostDetailed::default();
        let result = validate_marker_data(
            Timeline::Home,
            &post.id.to_string(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_marker_data_notifications() {
        let notification = NotificationDetailed::for_test();
        let result = validate_marker_data(
            Timeline::Notifications,
            &notification.id.to_string(),
        );
        assert!(result.is_ok());
    }
}
