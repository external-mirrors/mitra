use apx_core::http_types::{
    header,
    HeaderMap,
};

use super::constants::{AP_MEDIA_TYPE, AS_MEDIA_TYPE};
use super::utils::extract_media_type;

pub fn is_activitypub_request(
    headers: &HeaderMap,
) -> bool {
    let maybe_user_agent = headers.get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    if let Some(user_agent) = maybe_user_agent {
        if user_agent.contains("THIS. IS. GNU social!!!!") {
            // GNU Social doesn't send valid Accept headers
            return true;
        };
    };
    const MEDIA_TYPES: [&str; 4] = [
        AP_MEDIA_TYPE,
        AS_MEDIA_TYPE,
        "application/ld+json",
        "application/json",
    ];
    let media_type = headers.get(header::ACCEPT)
        .and_then(extract_media_type)
        .unwrap_or_default();
    MEDIA_TYPES.contains(&media_type.as_str())
}

#[cfg(test)]
mod tests {
    use apx_core::http_types::HeaderValue;
    use super::*;

    #[test]
    fn test_is_activitypub_request_activitypub() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static(AP_MEDIA_TYPE),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_activitypub_request_mastodon() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static(r#"application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams", text/html;q=0.1"#),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_activitypub_request_pleroma() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/activity+json"),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_activitypub_request_bridgy_fed() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/activity+json; q=0.9, application/ld+json;profile=\x22https://www.w3.org/ns/activitystreams\x22; q=0.8, text/html; charset=utf-8; q=0.7"),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_activitypub_request_browser() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("text/html"),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, false);
    }
}
