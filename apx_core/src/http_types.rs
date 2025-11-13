//! Re-exported `http` v1.0 types and adapters for `http` v0.2 types.

use std::str::FromStr;

use http_0_2;

pub use http::{
    header,
    HeaderMap,
    HeaderName,
    HeaderValue,
    Method,
    Uri,
};

pub fn header_name_adapter(value: &http_0_2::HeaderName) -> HeaderName {
    HeaderName::from_str(value.as_str())
        .expect("header name should be valid")
}

pub fn header_value_adapter(value: &http_0_2::HeaderValue) -> HeaderValue {
    HeaderValue::from_bytes(value.as_bytes())
        .expect("header value should be valid")
}

pub fn header_map_adapter<'m>(
    value: impl IntoIterator<Item = (&'m http_0_2::HeaderName, &'m http_0_2::HeaderValue)>,
) -> HeaderMap {
    let map_iter = value.into_iter().map(|(name, value)| {
        (header_name_adapter(name), header_value_adapter(value))
    });
    HeaderMap::from_iter(map_iter)
}

pub fn method_adapter(value: &http_0_2::Method) -> Method {
    Method::from_str(value.as_str())
        .expect("method name should be valid")
}

pub fn uri_adapter(value: &http_0_2::Uri) -> Uri {
    Uri::try_from(value.to_string())
        .expect("URI should be valid")
}
