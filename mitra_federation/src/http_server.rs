use http::{
    header,
    HeaderMap,
    HeaderName,
    HeaderValue,
};

use super::constants::{AP_MEDIA_TYPE, AS_MEDIA_TYPE};

// Compatible with:
// - http::HeaderMap
// - actix_web::http::header::HeaderMap
pub fn is_activitypub_request<'m>(
    headers: impl IntoIterator<Item = (&'m HeaderName, &'m HeaderValue)>,
) -> bool {
    // Create header map
    let headers = HeaderMap::from_iter(
        headers.into_iter()
            .map(|(name, val)| (name.clone(), val.clone())));

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
    if let Some(media_type) = headers.get(header::ACCEPT) {
        let media_type_str = media_type.to_str().ok()
            // Take first media type if there are many
            .and_then(|value| value.split(',').next())
            // Remove q parameter
            .map(|value| {
                value
                    .split(';')
                    .filter(|part| !part.contains("q="))
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .unwrap_or("".to_string());
        return MEDIA_TYPES.contains(&media_type_str.as_str());
    };
    false
}

#[cfg(test)]
mod tests {
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
