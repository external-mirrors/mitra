use std::collections::BTreeMap;

use actix_governor::{
    governor::middleware::NoOpMiddleware,
    GovernorConfig,
    GovernorConfigBuilder,
    PeerIpKeyExtractor,
};
use actix_web::{
    dev::{ConnectionInfo, ServiceResponse},
    error::{Error, JsonPayloadError},
    http::{
        header as http_header,
        header::HeaderMap as ActixHeaderMap,
    },
    middleware::DefaultHeaders,
    web::{Form, Json},
    Either,
    HttpRequest,
};
use log::Level;
use serde_qs::actix::{QsForm, QsQuery};

use apx_core::http_types::{header_map_adapter, HeaderMap};

use crate::errors::HttpError;

pub fn actix_header_map_adapter(header_map: &ActixHeaderMap) -> HeaderMap {
    header_map_adapter(header_map)
}

pub type FormOrJson<T> = Either<Form<T>, Json<T>>;
pub type QsFormOrJson<T> = Either<QsForm<T>, Json<T>>;

// actix currently doesn't support parameter arrays
// https://github.com/actix/actix-web/issues/2044
pub type MultiQuery<T> = QsQuery<T>;

pub type RatelimitConfig = GovernorConfig<PeerIpKeyExtractor, NoOpMiddleware>;

pub fn ratelimit_config(
    num_requests: u32,
    period: u64,
    permissive: bool,
) -> RatelimitConfig {
    GovernorConfigBuilder::default()
        .burst_size(num_requests)
        .seconds_per_request(period)
        .permissive(permissive)
        .finish()
        .expect("governor parameters should be non-zero")
}

pub struct ContentSecurityPolicy {
    directives: BTreeMap<String, String>,
}

impl ContentSecurityPolicy {
    pub fn insert(&mut self, directive: &str, value: &str) -> () {
        self.directives.insert(directive.to_string(), value.to_string());
    }

    pub fn into_string(self) -> String {
        self.directives.iter()
            .map(|(key, val)| format!("{key} {val}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

}

impl Default for ContentSecurityPolicy {
    fn default() -> Self {
        let defaults = [
            ("default-src", "'none'"),
            ("connect-src", "'self'"),
            ("img-src", "'self' data:"),
            ("media-src", "'self'"),
            // script-src unsafe-inline required by MetaMask
            // https://github.com/MetaMask/metamask-extension/issues/3133
            ("script-src", "'self' 'unsafe-inline'"),
            ("style-src", "'self'"),
            ("manifest-src", "'self'"),
            ("frame-ancestors", "'none'"),
            ("base-uri", "'self'"),
            // form-action doesn't work properly in Chrome.
            // Redirects are blocked even if scheme is whitelisted.
            //("form-action", "'self'"),
        ];
        let directives = defaults.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Self { directives }
    }
}

pub fn create_default_headers_middleware() -> DefaultHeaders {
    DefaultHeaders::new()
        .add((
            http_header::CONTENT_SECURITY_POLICY,
            ContentSecurityPolicy::default().into_string(),
        ))
        .add((http_header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
}

pub fn log_response_error<B>(
    level: Level,
    response: &ServiceResponse<B>,
) -> () {
    if let Some(error) = response.response().error() {
        log::log!(
            level,
            "{} {} : {}",
            response.request().method(),
            response.request().path(),
            error,
        );
    };
}

/// Convert JSON payload deserialization errors into validation errors
pub fn json_error_handler(
    error: JsonPayloadError,
    _: &HttpRequest,
) -> Error {
    match error {
        JsonPayloadError::Deserialize(de_error) => {
            HttpError::ValidationError(de_error.to_string()).into()
        },
        other_error => other_error.into(),
    }
}

pub fn get_request_base_url(connection_info: ConnectionInfo) -> String {
    let scheme = connection_info.scheme();
    let host = connection_info.host();
    format!("{}://{}", scheme, host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_security_policy() {
        let csp = ContentSecurityPolicy::default();
        assert_eq!(
            csp.into_string(),
            "base-uri 'self'; connect-src 'self'; default-src 'none'; frame-ancestors 'none'; img-src 'self' data:; manifest-src 'self'; media-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self'",
        );
    }
}
