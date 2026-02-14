use actix_web::{http::Uri, HttpResponse};
use serde::{Deserialize, Serialize};
use url::Url;

fn get_pagination_header(
    base_url: &str,
    request_uri: &Uri,
    last_id: &str,
) -> String {
    let mut next_page_url: Url = base_url.parse()
        .expect("should be valid URL");
    next_page_url.set_path(request_uri.path());
    next_page_url.set_query(request_uri.query());
    // Remove max_id from query pairs and append new value
    let query_pairs: Vec<_> = next_page_url
        .query_pairs()
        .into_owned()
        .filter(|(key, _value)| key != "max_id")
        .collect();
    next_page_url
        .query_pairs_mut()
        .clear()
        .extend_pairs(query_pairs)
        .append_pair("max_id", last_id);
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Link
    format!(r#"<{}>; rel="next""#, next_page_url)
}

pub fn get_paginated_response(
    base_url: &str,
    request_uri: &Uri,
    items: Vec<impl Serialize>,
    maybe_last_item_id: Option<impl ToString>,
) -> HttpResponse {
    if let Some(last_item_id) = maybe_last_item_id {
        let pagination_header = get_pagination_header(
            base_url,
            request_uri,
            &last_item_id.to_string(),
        );
        HttpResponse::Ok()
            .append_header(("Link", pagination_header))
            .json(items)
    } else {
        HttpResponse::Ok().json(items)
    }
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "u16")]
pub struct PageSize(u16);

impl PageSize {
    pub const MAX: u16 = 200;

    pub fn new(size: u16) -> Self { Self(size) }

    pub fn inner(&self) -> u16 { self.0 }
}

impl TryFrom<u16> for PageSize {
    type Error = &'static str;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err("expected an integer between 0 and 201")
        }
    }
}

pub fn get_last_item<'item, T>(
    items: &'item [T],
    limit: &PageSize,
) -> Option<&'item T> {
    let max_index = usize::from(limit.inner().saturating_sub(1));
    items.get(max_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URI: &str = "https://example.org";

    #[test]
    fn test_get_next_page_link() {
        let request_uri =
            Uri::from_static("/api/v1/notifications?max_id=103");
        let result = get_pagination_header(
            INSTANCE_URI,
            &request_uri,
            "123",
        );
        assert_eq!(
            result,
            r#"<https://example.org/api/v1/notifications?max_id=123>; rel="next""#,
        );
    }

    #[test]
    fn test_deserialize_page_size() {
        let value: PageSize = serde_json::from_str("10").unwrap();
        assert_eq!(value.inner(), 10);
        let value: PageSize = serde_json::from_str("0").unwrap();
        assert_eq!(value.inner(), 0);

        let expected_error = "invalid value: integer `-1`, expected u16 at line 1 column 2";
        let error = serde_json::from_str::<PageSize>("-1").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
        let expected_error = "expected an integer between 0 and 201";
        let error = serde_json::from_str::<PageSize>("201").unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }
}
