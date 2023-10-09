use actix_governor::{
    GovernorConfig,
    GovernorConfigBuilder,
    PeerIpKeyExtractor,
};
use actix_web::{
    body::{BodySize, BoxBody, MessageBody},
    dev::{ConnectionInfo, ServiceResponse},
    error::{Error, JsonPayloadError},
    http::StatusCode,
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
    actix::{QsQuery, QsQueryConfig},
    Config as QsConfig,
};

use crate::errors::HttpError;

pub type FormOrJson<T> = Either<Form<T>, Json<T>>;

// actix currently doesn't support parameter arrays
// https://github.com/actix/actix-web/issues/2044
pub type MultiQuery<T> = QsQuery<T>;

pub fn multiquery_config() -> QsQueryConfig {
    // Disable strict mode
    let qs_config = QsConfig::new(2, false);
    QsQueryConfig::default().qs_config(qs_config)
}

pub type RatelimitConfig = GovernorConfig<PeerIpKeyExtractor>;

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

pub fn create_default_headers_middleware() -> DefaultHeaders {
    DefaultHeaders::new()
        .add((
            "Content-Security-Policy",
            // script-src unsafe-inline required by MetaMask
            // style-src oauth-authorization required by OAuth authorization page
            "default-src 'none'; \
                connect-src 'self'; \
                img-src 'self' data:; \
                media-src 'self'; \
                script-src 'self' 'unsafe-inline'; \
                style-src 'self' 'nonce-oauth-authorization'; \
                manifest-src 'self'; \
                frame-ancestors 'none'; \
                base-uri 'self'; \
                form-action 'self'",
        ))
        .add(("X-Content-Type-Options", "nosniff"))
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
