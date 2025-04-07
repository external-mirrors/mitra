//! Verify HTTP signatures
use std::collections::HashMap;

use chrono::{DateTime, Duration, TimeZone, Utc};
use http::{HeaderMap, Method, Uri};
use regex::Regex;

use crate::{
    base64,
    crypto::common::PublicKey,
    crypto_eddsa::verify_eddsa_signature,
    crypto_rsa::verify_rsa_sha256_signature,
    http_digest::{parse_digest_header, ContentDigest},
    http_url::HttpUrl,
    http_utils::remove_quotes,
};

const SIGNATURE_PARAMETER_RE: &str = r#"^(?P<key>[a-zA-Z]+)=(?P<value>.+)$"#;

const SIGNATURE_EXPIRES_IN: i64 = 12; // 12 hours

#[derive(thiserror::Error, Debug)]
pub enum HttpSignatureVerificationError {
    #[error("HTTP method not supported")]
    MethodNotSupported,

    #[error("missing signature header")]
    NoSignature,

    #[error("{0}")]
    HeaderError(&'static str),

    #[error("{0}")]
    ParseError(&'static str),

    #[error("invalid encoding")]
    InvalidEncoding(#[from] base64::DecodeError),

    #[error("signature has expired")]
    Expired,

    #[error("digest mismatch")]
    DigestMismatch,

    #[error("invalid signature")]
    InvalidSignature,
}

type VerificationError = HttpSignatureVerificationError;

pub struct HttpSignatureData {
    pub key_id: HttpUrl,
    pub message: String, // reconstructed message
    pub signature: String, // base64-encoded signature
    pub expires_at: DateTime<Utc>,
    pub content_digest: Option<ContentDigest>,
}

pub fn parse_http_signature(
    request_method: &Method,
    request_uri: &Uri,
    request_headers: &HeaderMap,
) -> Result<HttpSignatureData, VerificationError> {
    // Parse Digest header
    let maybe_digest = match *request_method {
        Method::GET => None,
        Method::POST => {
            let digest_header = request_headers.get("digest")
                .ok_or(VerificationError::HeaderError("missing 'digest' header"))?
                .to_str()
                .map_err(|_| VerificationError::HeaderError("invalid 'digest' header"))?;
            let digest = parse_digest_header(digest_header)
                .map_err(VerificationError::HeaderError)?;
            Some(digest)
        },
        _ => return Err(VerificationError::MethodNotSupported),
    };

    // Parse Signature header
    let signature_header = request_headers.get("signature")
        .ok_or(VerificationError::NoSignature)?
        .to_str()
        .map_err(|_| VerificationError::HeaderError("invalid signature header"))?;

    let signature_parameter_re = Regex::new(SIGNATURE_PARAMETER_RE)
        .expect("regexp should be valid");
    let mut signature_parameters = HashMap::new();
    for item in signature_header.split(',') {
        let caps = signature_parameter_re.captures(item)
            .ok_or(VerificationError::HeaderError("invalid signature header"))?;
        let key = caps["key"].to_string();
        let value = remove_quotes(&caps["value"]);
        signature_parameters.insert(key, value);
    };

    let key_id_str = signature_parameters.get("keyId")
        .ok_or(VerificationError::ParseError("keyId parameter is missing"))?;
    let key_id = HttpUrl::parse(key_id_str)
        .map_err(|_| VerificationError::ParseError("invalid key ID"))?;
    let headers_parameter = signature_parameters.get("headers")
        .ok_or(VerificationError::ParseError("headers parameter is missing"))?
        .to_owned();
    let signature = signature_parameters.get("signature")
        .ok_or(VerificationError::ParseError("signature is missing"))?
        .to_owned();
    let created_at = if let Some(created_at) = signature_parameters.get("created") {
        let create_at_timestamp = created_at.parse()
            .map_err(|_| VerificationError::ParseError("invalid timestamp"))?;
        Utc.timestamp_opt(create_at_timestamp, 0).single()
            .ok_or(VerificationError::ParseError("invalid timestamp"))?
    } else {
        let date_str = request_headers.get("date")
            .ok_or(VerificationError::HeaderError("missing date"))?
            .to_str()
            .map_err(|_| VerificationError::HeaderError("invalid date header"))?;
        let date = DateTime::parse_from_rfc2822(date_str)
            .map_err(|_| VerificationError::HeaderError("invalid date"))?;
        date.with_timezone(&Utc)
    };
    let expires_at = if let Some(expires_at) = signature_parameters.get("expires") {
        let expires_at_timestamp = expires_at.parse()
            .map_err(|_| VerificationError::ParseError("invalid timestamp"))?;
        Utc.timestamp_opt(expires_at_timestamp, 0).single()
            .ok_or(VerificationError::ParseError("invalid timestamp"))?
    } else {
        created_at + Duration::hours(SIGNATURE_EXPIRES_IN)
    };

    let mut message_parts = vec![];
    for header in headers_parameter.split(' ') {
        let message_part = if header == "(request-target)" {
            format!(
                "(request-target): {} {}",
                request_method.as_str().to_lowercase(),
                request_uri.path(),
            )
        } else if header == "(created)" {
            let created = signature_parameters.get("created")
                .ok_or(VerificationError::ParseError("created parameter is missing"))?;
            format!("(created): {}", created)
        } else if header == "(expires)" {
            let expires = signature_parameters.get("expires")
                .ok_or(VerificationError::ParseError("expires parameter is missing"))?;
            format!("(expires): {}", expires)
        } else {
            let header_value = request_headers.get(header)
                .ok_or(VerificationError::HeaderError("missing header"))?
                .to_str()
                .map_err(|_| VerificationError::HeaderError("invalid header value"))?;
            format!("{}: {}", header, header_value)
        };
        message_parts.push(message_part);
    };
    let message = message_parts.join("\n");

    let signature_data = HttpSignatureData {
        key_id,
        message,
        signature,
        expires_at,
        content_digest: maybe_digest,
    };
    Ok(signature_data)
}

pub fn verify_http_signature(
    signature_data: &HttpSignatureData,
    signer_key: &PublicKey,
    content_digest: Option<ContentDigest>,
) -> Result<(), VerificationError> {
    if signature_data.expires_at < Utc::now() {
        return Err(VerificationError::Expired);
    };
    if signature_data.content_digest != content_digest {
        return Err(VerificationError::DigestMismatch);
    };
    let signature = base64::decode(&signature_data.signature)?;
    let is_valid_signature = match signer_key {
        PublicKey::Ed25519(ed25519_key) => {
            verify_eddsa_signature(
                ed25519_key,
                signature_data.message.as_bytes(),
                &signature,
            ).is_ok()
        },
        PublicKey::Rsa(rsa_key) => {
            verify_rsa_sha256_signature(
                rsa_key,
                signature_data.message.as_bytes(),
                &signature,
            ).is_ok()
        },
    };
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use http::{HeaderName, HeaderValue};
    use crate::{
        crypto_eddsa::generate_weak_ed25519_key,
        crypto_rsa::generate_weak_rsa_key,
        http_signatures::create::{
            create_http_signature,
            HttpSigner,
        },
    };
    use super::*;

    #[test]
    fn test_parse_signature_get() {
        let request_method = Method::GET;
        let request_uri = "/user/123/inbox".parse::<Uri>().unwrap();
        let date = "20 Oct 2022 20:00:00 GMT";
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_static("example.com"),
        );
        request_headers.insert(
            HeaderName::from_static("date"),
            HeaderValue::from_str(&date).unwrap(),
        );
        let signature_header = concat!(
            r#"keyId="https://myserver.org/actor#main-key","#,
            r#"algorithm=hs2019,"#,
            r#"headers="(request-target) host date","#,
            r#"signature="test""#,
        );
        request_headers.insert(
            HeaderName::from_static("signature"),
            HeaderValue::from_static(signature_header),
        );

        let signature_data = parse_http_signature(
            &request_method,
            &request_uri,
            &request_headers,
        ).unwrap();
        assert_eq!(
            signature_data.key_id.as_str(),
            "https://myserver.org/actor#main-key",
        );
        assert_eq!(
            signature_data.message,
            "(request-target): get /user/123/inbox\nhost: example.com\ndate: 20 Oct 2022 20:00:00 GMT",
        );
        assert_eq!(signature_data.signature, "test");
        assert!(signature_data.expires_at < Utc::now());
        assert!(signature_data.content_digest.is_none());
    }

    #[test]
    fn test_create_and_verify_signature_get() {
        let request_method = Method::GET;
        let request_url = "https://example.org/inbox";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://myserver.org/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);
        let signed_headers = create_http_signature(
            request_method.clone(),
            request_url,
            b"",
            &signer,
        ).unwrap();

        let request_url = request_url.parse::<Uri>().unwrap();
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_str(&signed_headers.host).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("signature"),
            HeaderValue::from_str(&signed_headers.signature).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("date"),
            HeaderValue::from_str(&signed_headers.date).unwrap(),
        );
        let signature_data = parse_http_signature(
            &request_method,
            &request_url,
            &request_headers,
        ).unwrap();
        assert_eq!(signature_data.content_digest.is_some(), false);

        let signer_public_key = signer.key.public_key();
        let result = verify_http_signature(
            &signature_data,
            &signer_public_key,
            None,
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_signature_post() {
        let request_method = Method::POST;
        let request_url = "https://example.org/inbox";
        let request_body = "{}";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://myserver.org/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);
        let signed_headers = create_http_signature(
            request_method.clone(),
            request_url,
            request_body.as_bytes(),
            &signer,
        ).unwrap();

        let request_url = request_url.parse::<Uri>().unwrap();
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_str(&signed_headers.host).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("signature"),
            HeaderValue::from_str(&signed_headers.signature).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("date"),
            HeaderValue::from_str(&signed_headers.date).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("digest"),
            HeaderValue::from_str(&signed_headers.digest.unwrap()).unwrap(),
        );
        let signature_data = parse_http_signature(
            &request_method,
            &request_url,
            &request_headers,
        ).unwrap();
        assert_eq!(signature_data.content_digest.is_some(), true);

        let signer_public_key = signer.key.public_key();
        let content_digest = ContentDigest::new(request_body.as_bytes());
        let result = verify_http_signature(
            &signature_data,
            &signer_public_key,
            Some(content_digest),
        );
        assert_eq!(result.is_ok(), true);
    }

    #[test]
    fn test_create_and_verify_signature_post_eddsa() {
        let request_method = Method::POST;
        let request_url = "https://server.example/inbox";
        let request_body = "{}";
        let signer_key = generate_weak_ed25519_key();
        let signer_key_id = "https://myserver.org/actor#ed25519-key".to_string();
        let signer = HttpSigner::new_ed25519(signer_key, signer_key_id);
        let signed_headers = create_http_signature(
            request_method.clone(),
            request_url,
            request_body.as_bytes(),
            &signer,
        ).unwrap();

        let request_url = request_url.parse::<Uri>().unwrap();
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_str(&signed_headers.host).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("signature"),
            HeaderValue::from_str(&signed_headers.signature).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("date"),
            HeaderValue::from_str(&signed_headers.date).unwrap(),
        );
        request_headers.insert(
            HeaderName::from_static("digest"),
            HeaderValue::from_str(&signed_headers.digest.unwrap()).unwrap(),
        );
        let signature_data = parse_http_signature(
            &request_method,
            &request_url,
            &request_headers,
        ).unwrap();
        assert_eq!(signature_data.content_digest.is_some(), true);

        let signer_public_key = signer.key.public_key();
        let content_digest = ContentDigest::new(request_body.as_bytes());
        let result = verify_http_signature(
            &signature_data,
            &signer_public_key,
            Some(content_digest),
        );
        assert_eq!(result.is_ok(), true);
    }
}
