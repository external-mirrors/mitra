use std::collections::BTreeMap;

use actix_governor::{
    governor::middleware::NoOpMiddleware,
    GovernorConfig,
    GovernorConfigBuilder,
};
use actix_web::{
    body::MessageBody,
    dev::{ConnectionInfo, ServiceResponse},
    error::{Error, JsonPayloadError},
    http::{
        header as http_header,
        Uri,
    },
    middleware::DefaultHeaders,
    web::{Form, Json},
    Either,
    HttpRequest,
};
use log::Level;
use serde_qs::actix::{QsForm, QsQuery};

use crate::{
    errors::HttpError,
    ratelimit::RealIpKeyExtractor,
};

pub type FormOrJson<T> = Either<Form<T>, Json<T>>;
pub type QsFormOrJson<T> = Either<QsForm<T>, Json<T>>;

// actix currently doesn't support parameter arrays
// https://github.com/actix/actix-web/issues/2044
pub type MultiQuery<T> = QsQuery<T>;

pub type RatelimitConfig = GovernorConfig<RealIpKeyExtractor, NoOpMiddleware>;

pub fn ratelimit_config(
    num_requests: u32,
    period: u64,
    permissive: bool,
) -> RatelimitConfig {
    GovernorConfigBuilder::default()
        .key_extractor(RealIpKeyExtractor)
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
            // External connections are required for "attach from URL" feature
            ("connect-src", "'self' *"),
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

pub fn log_response_error<B: MessageBody>(
    level: Level,
    response: &ServiceResponse<B>,
) -> () {
    let error_message = if let Some(error) = response.response().error() {
        // Actix error
        error.to_string()
    } else {
        response.response()
            .status().canonical_reason()
            .unwrap_or("unknown error")
            .to_owned()
    };
    log::log!(
        level,
        "{} {} : {}",
        response.request().method(),
        response.request().path(),
        error_message,
    );
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

fn _get_request_full_uri(
    connection_scheme: &str,
    connection_host: &str,
    request_uri: &Uri,
) -> Option<Uri> {
    let scheme_normalized = connection_scheme.to_lowercase();
    let authority_normalized = connection_host.to_lowercase();
    let path_and_query = request_uri.path_and_query()
        .map(|paq| paq.as_str())
        .unwrap_or("/");
    let uri = Uri::builder()
        .scheme(scheme_normalized.as_str())
        .authority(authority_normalized)
        .path_and_query(path_and_query)
        .build()
        .ok()?;
    Some(uri)
}

// Similar to HttpRequest::full_url, but returns Uri
// https://docs.rs/actix-web/4.8.0/actix_web/struct.HttpRequest.html#method.full_url
pub fn get_request_full_uri(
    connection_info: &ConnectionInfo,
    request_uri: &Uri,
) -> Uri {
    let scheme = connection_info.scheme();
    // Host is expected to be URI-compatible
    // https://httpwg.org/specs/rfc9110.html#field.host
    let host = connection_info.host();
    _get_request_full_uri(scheme, host, request_uri)
        .unwrap_or_else(|| {
            log::error!(
                "can't construct full URI from {} {} and {}",
                scheme,
                host,
                request_uri,
            );
            request_uri.clone()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_security_policy() {
        let csp = ContentSecurityPolicy::default();
        assert_eq!(
            csp.into_string(),
            "base-uri 'self'; connect-src 'self' *; default-src 'none'; frame-ancestors 'none'; img-src 'self' data:; manifest-src 'self'; media-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self'",
        );
    }

    #[test]
    fn test_get_request_full_uri() {
        let connection_scheme = "HTTPS";
        let connection_host = "SOCIAL.EXAMPLE";
        let request_uri = Uri::from_static("/inbox");
        let full_uri = _get_request_full_uri(
            connection_scheme,
            connection_host,
            &request_uri,
        ).unwrap();
        assert_eq!(full_uri.to_string(), "https://social.example/inbox");
    }
}
