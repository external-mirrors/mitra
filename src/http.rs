use std::collections::BTreeMap;

use actix_governor::{
    governor::middleware::NoOpMiddleware,
    GovernorConfig,
    GovernorConfigBuilder,
    PeerIpKeyExtractor,
};
use actix_web::{
    body::{BodySize, BoxBody, MessageBody},
    dev::{ConnectionInfo, ServiceResponse},
    error::{Error, JsonPayloadError},
    http::{
        header as http_header,
        StatusCode,
    },
    middleware::{
        DefaultHeaders,
        ErrorHandlerResponse,
        ErrorHandlers,
    },
    web::{Form, Json},
    Either,
    HttpRequest,
};
use serde_json::json;
use serde_qs::{
    actix::{QsForm, QsFormConfig, QsQuery, QsQueryConfig},
    Config as QsConfig,
};

use crate::errors::HttpError;

pub type FormOrJson<T> = Either<Form<T>, Json<T>>;
pub type QsFormOrJson<T> = Either<QsForm<T>, Json<T>>;

// actix currently doesn't support parameter arrays
// https://github.com/actix/actix-web/issues/2044
pub type MultiQuery<T> = QsQuery<T>;

pub fn multiquery_config() -> QsQueryConfig {
    // Disable strict mode
    let qs_config = QsConfig::new(2, false);
    QsQueryConfig::default().qs_config(qs_config)
}

pub fn multiquery_form_config() -> QsFormConfig {
    // Disable strict mode
    let qs_config = QsConfig::new(2, false);
    QsFormConfig::default().qs_config(qs_config)
}

pub type RatelimitConfig = GovernorConfig<PeerIpKeyExtractor, NoOpMiddleware>;

pub fn ratelimit_config(num_requests: u32, period: u64) -> RatelimitConfig {
    GovernorConfigBuilder::default()
        .per_second(period)
        .burst_size(num_requests)
        .finish()
        .expect("governor parameters should be non-zero")
}

/// Error handler for 401 Unauthorized
pub fn create_auth_error_handler<B: MessageBody + 'static>() -> ErrorHandlers<B> {
    // Creates and returns actix middleware
    ErrorHandlers::new()
        .handler(StatusCode::UNAUTHORIZED, |response: ServiceResponse<B>| {
            let response_new = response.map_body(|_, body| {
                if let BodySize::None | BodySize::Sized(0) = body.size() {
                    // Insert error description if response body is empty
                    // https://github.com/actix/actix-extras/issues/156
                    let error_data = json!({
                        "message": "auth header is not present",
                    });
                    return BoxBody::new(error_data.to_string());
                };
                body.boxed()
            });
            Ok(ErrorHandlerResponse::Response(response_new.map_into_right_body()))
        })
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
