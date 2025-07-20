//! Verify HTTP signatures
use std::collections::HashMap;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use http::{HeaderMap, Method, Uri};
use indexmap::IndexMap;
use regex::Regex;
use sfv::{
    BareItem,
    Item,
    ListEntry,
    Parser,
    SerializeValue,
};

use crate::{
    base64,
    crypto::common::PublicKey,
    crypto_eddsa::verify_eddsa_signature,
    crypto_rsa::verify_rsa_sha256_signature,
    http_digest::{
        parse_digest_header,
        parse_content_digest_header,
        ContentDigest,
    },
    http_utils::remove_quotes,
};

pub use crate::json_signatures::verify::VerificationMethod;

const SIGNATURE_PARAMETER_RE: &str = r#"^(?P<key>[a-zA-Z]+)=(?P<value>.+)$"#;

const SIGNATURE_EXPIRES_IN: i64 = 12; // 12 hours

#[derive(thiserror::Error, Debug)]
pub enum HttpSignatureVerificationError {
    #[error("HTTP method not supported")]
    MethodNotSupported,

    #[error("missing signature header")]
    NoSignature,

    #[error("{1}: {0}")]
    HeaderError(String, &'static str),

    #[error("{0}")]
    ParseError(&'static str),

    #[error("invalid encoding")]
    InvalidEncoding(#[from] base64::DecodeError),

    #[error("signature has expired")]
    Expired,

    #[error("missing content digest")]
    NoDigest,

    #[error("digest mismatch")]
    DigestMismatch,

    #[error("invalid signature")]
    InvalidSignature,
}

impl HttpSignatureVerificationError {
    fn header_missing(header_name: &str) -> Self {
        Self::HeaderError(header_name.to_owned(), "missing header")
    }

    fn header_value(header_name: &str) -> Self {
        Self::HeaderError(header_name.to_owned(), "invalid header value")
    }
}

type VerificationError = HttpSignatureVerificationError;

pub struct HttpSignatureData {
    pub is_rfc9421: bool,
    pub key_id: VerificationMethod,
    pub base: String, // recreated signature base
    pub signature: Vec<u8>,
    pub expires_at: DateTime<Utc>,
    pub content_digest: Option<ContentDigest>,
}

fn get_content_digest(
    headers: &HeaderMap,
) -> Result<Option<ContentDigest>, VerificationError> {
    let maybe_digest = if let Some(header_value) = headers.get("Content-Digest") {
        let header_value = header_value
            .to_str()
            .map_err(|_| VerificationError::header_value("Content-Digest"))?;
        let content_digest = parse_content_digest_header(header_value)
            .map_err(|error| VerificationError::HeaderError(
                "Content-Digest".to_owned(),
                error,
            ))?;
        Some(content_digest)
    } else if let Some(header_value) = headers.get("Digest") {
        let header_value = header_value
            .to_str()
            .map_err(|_| VerificationError::header_value("Digest"))?;
        let content_digest = parse_digest_header(header_value)
            .map_err(|error| VerificationError::HeaderError(
                "Digest".to_owned(),
                error,
            ))?;
        Some(content_digest)
    } else {
        None
    };
    Ok(maybe_digest)
}

/// Parses Draft-Cavage HTTP signature  
/// <https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures>
pub fn parse_http_signature_cavage(
    request_method: &Method,
    request_uri: &Uri,
    request_headers: &HeaderMap,
) -> Result<HttpSignatureData, VerificationError> {
    // Parse Digest header
    let maybe_digest = match *request_method {
        Method::GET => None,
        Method::POST => {
            let digest = get_content_digest(request_headers)?
                .ok_or(VerificationError::NoDigest)?;
            Some(digest)
        },
        _ => return Err(VerificationError::MethodNotSupported),
    };

    // Parse Signature header
    let signature_header = request_headers.get("signature")
        .ok_or(VerificationError::NoSignature)?
        .to_str()
        .map_err(|_| VerificationError::header_value("Signature"))?;

    let signature_parameter_re = Regex::new(SIGNATURE_PARAMETER_RE)
        .expect("regexp should be valid");
    let mut signature_parameters = HashMap::new();
    for item in signature_header.split(',') {
        let caps = signature_parameter_re.captures(item)
            .ok_or(VerificationError::header_value("Signature"))?;
        let key = caps["key"].to_string();
        let value = remove_quotes(&caps["value"]);
        signature_parameters.insert(key, value);
    };

    let key_id_str = signature_parameters.get("keyId")
        .ok_or(VerificationError::ParseError("keyId parameter is missing"))?;
    let key_id = VerificationMethod::parse(key_id_str)
        .map_err(|_| VerificationError::ParseError("invalid key ID"))?;
    let headers_parameter = signature_parameters.get("headers")
        .ok_or(VerificationError::ParseError("headers parameter is missing"))?
        .to_owned();
    let signature_b64 = signature_parameters.get("signature")
        .ok_or(VerificationError::ParseError("signature is missing"))?;
    let signature = base64::decode(signature_b64)?;
    let created_at = if let Some(created_at) = signature_parameters.get("created") {
        let create_at_timestamp = created_at.parse()
            .map_err(|_| VerificationError::ParseError("invalid timestamp"))?;
        Utc.timestamp_opt(create_at_timestamp, 0).single()
            .ok_or(VerificationError::ParseError("invalid timestamp"))?
    } else {
        let date_str = request_headers.get("date")
            .ok_or(VerificationError::header_missing("Date"))?
            .to_str()
            .map_err(|_| VerificationError::header_value("Date"))?;
        let date = DateTime::parse_from_rfc2822(date_str)
            .map_err(|_| VerificationError::header_value("Date"))?;
        date.with_timezone(&Utc)
    };
    let expires_at = if let Some(expires_at) = signature_parameters.get("expires") {
        let expires_at_timestamp = expires_at.parse()
            .map_err(|_| VerificationError::ParseError("invalid timestamp"))?;
        Utc.timestamp_opt(expires_at_timestamp, 0).single()
            .ok_or(VerificationError::ParseError("invalid timestamp"))?
    } else {
        created_at + TimeDelta::hours(SIGNATURE_EXPIRES_IN)
    };

    // Recreate signature base
    let mut signature_base_entries = IndexMap::new();
    for header in headers_parameter.split(' ') {
        let header_value = if header == "(request-target)" {
            format!(
                "{} {}",
                request_method.as_str().to_lowercase(),
                request_uri.path(),
            )
        } else if header == "(created)" {
            signature_parameters.get("created")
                .ok_or(VerificationError::ParseError("created parameter is missing"))?
                .clone()
        } else if header == "(expires)" {
            signature_parameters.get("expires")
                .ok_or(VerificationError::ParseError("expires parameter is missing"))?
                .clone()
        } else {
            request_headers.get(header)
                .ok_or(VerificationError::header_missing(header))?
                .to_str()
                .map_err(|_| VerificationError::header_value(header))?
                .to_owned()
        };
        signature_base_entries.insert(header, header_value);
    };
    let signature_base = signature_base_entries
        .into_iter()
        .map(|(id, value)| format!("{id}: {value}"))
        .collect::<Vec<_>>()
        .join("\n");

    let signature_data = HttpSignatureData {
        key_id,
        base: signature_base,
        signature,
        expires_at,
        content_digest: maybe_digest,
        is_rfc9421: false,
    };
    Ok(signature_data)
}

/// Parses RFC-9421 HTTP message signature  
/// <https://datatracker.ietf.org/doc/html/rfc9421>
pub fn parse_http_signature_rfc9421(
    request_method: &Method,
    request_uri: &Uri,
    request_headers: &HeaderMap,
) -> Result<HttpSignatureData, VerificationError> {
    // Parse Signature header
    let signature_header = request_headers.get("Signature")
        .ok_or(VerificationError::NoSignature)?
        .to_str()
        .map_err(|_| VerificationError::header_value("Signature"))?;
    let signature_dict = Parser::parse_dictionary(signature_header.as_bytes())
        .map_err(|_| VerificationError::ParseError("invalid 'signature' dictionary"))?;
    let (signature_label, signature_value_item) = signature_dict.first()
        .ok_or(VerificationError::ParseError("invalid 'signature' dictionary"))?;
    let signature_value = match signature_value_item {
        ListEntry::Item(Item { bare_item: BareItem::ByteSeq(value), .. }) => {
            value.clone()
        },
        _ => return Err(VerificationError::ParseError("invalid signature value")),
    };

    // Parse Signature-Input header
    let signature_input_header = request_headers.get("Signature-Input")
        .ok_or(VerificationError::NoSignature)?
        .to_str()
        .map_err(|_| VerificationError::header_value("Signature-Input"))?;
    let signature_input_dict = Parser::parse_dictionary(signature_input_header.as_bytes())
        .map_err(|_| VerificationError::ParseError("invalid 'signature-input' dictionary"))?;
    let signature_param_list = signature_input_dict.get(signature_label)
        .ok_or(VerificationError::ParseError("signature parameters not found"))?;
    let signature_params = vec![signature_param_list.clone()]
        .serialize_value()
        .map_err(|_| VerificationError::ParseError("serialization error"))?;
    let ListEntry::InnerList(signature_param_list) = signature_param_list else {
        return Err(VerificationError::ParseError("invalid encoding of signature parameters"));
    };
    let key_id = signature_param_list.params.get("keyid")
        .ok_or(VerificationError::ParseError("parameter 'keyid' not found"))?
        .as_str()
        .ok_or(VerificationError::ParseError("invalid encoding of 'keyid'"))?
        .to_owned();
    let key_id = VerificationMethod::parse(&key_id)
        .map_err(|_| VerificationError::ParseError("invalid key ID"))?;
    let expires_at = if let Some(expires_item) = signature_param_list.params.get("expires") {
        let expires = expires_item.as_int()
            .ok_or(VerificationError::ParseError("invalid encoding of 'expires'"))?;
        Utc.timestamp_opt(expires, 0).single()
            .ok_or(VerificationError::ParseError("invalid timestamp"))?
    } else {
        let created = signature_param_list.params.get("created")
            .ok_or(VerificationError::ParseError("parameter 'created' not found"))?
            .as_int()
            .ok_or(VerificationError::ParseError("invalid encoding of 'created'"))?;
        let created_at = Utc.timestamp_opt(created, 0).single()
            .ok_or(VerificationError::ParseError("invalid timestamp"))?;
        created_at + TimeDelta::hours(SIGNATURE_EXPIRES_IN)
    };
    let mut components = vec![];
    for list_item in &signature_param_list.items {
        let component_id = list_item.bare_item.as_str()
            .ok_or(VerificationError::ParseError("invalid encoding of signature parameter"))?
            .to_owned();
        components.push(component_id);
    };

    // Recreate signature base
    let mut signature_base_entries = IndexMap::new();
    for component_id in components.iter() {
        if signature_base_entries.contains_key(component_id.as_str()) {
            return Err(VerificationError::ParseError("duplicate component"));
        };
        let component_value = match component_id.as_str() {
            "@method" => {
                request_method.to_string()
            },
            "@target-uri" => {
                request_uri.to_string()
            },
            "@path" => {
                request_uri.path().to_owned()
            },
            "@query" => {
                // https://datatracker.ietf.org/doc/html/rfc9421#name-query
                let query = request_uri.query().unwrap_or_default();
                format!("?{query}")
            },
            "@authority" => {
                request_headers.get("host")
                    .ok_or(VerificationError::header_missing("Host"))?
                    .to_str()
                    .map_err(|_| VerificationError::header_value("Host"))?
                    .to_owned()
            },
            id if id.starts_with('@') => {
                return Err(VerificationError::ParseError("unsupported component ID"));
            },
            _ => {
                request_headers.get(component_id)
                    .ok_or(VerificationError::header_missing(component_id))?
                    .to_str()
                    .map_err(|_| VerificationError::header_value(component_id))?
                    .to_owned()
            },
        };
        signature_base_entries.insert(component_id.as_str(), component_value);
    };
    signature_base_entries.insert("@signature-params", signature_params);
    let signature_base = signature_base_entries
        .into_iter()
        .map(|(id, value)| format!(r#""{id}": {value}"#))
        .collect::<Vec<_>>()
        .join("\n");

    // Parse Digest header
    let maybe_digest = match *request_method {
        Method::GET => None,
        Method::POST => {
            let digest = get_content_digest(request_headers)?
                .ok_or(VerificationError::NoDigest)?;
            Some(digest)
        },
        _ => return Err(VerificationError::MethodNotSupported),
    };

    let signature_data = HttpSignatureData {
        key_id,
        base: signature_base,
        signature: signature_value,
        expires_at,
        content_digest: maybe_digest,
        is_rfc9421: true,
    };
    Ok(signature_data)
}

/// Parses Draft-Cavage HTTP signature or RFC-9421 HTTP message signature
pub fn parse_http_signature(
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
) -> Result<HttpSignatureData, VerificationError> {
    if headers.get("Signature-Input").is_some() {
        parse_http_signature_rfc9421(method, uri, headers)
    } else {
        parse_http_signature_cavage(method, uri, headers)
    }
}

/// Verifies HTTP signature
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
    let is_valid_signature = match signer_key {
        PublicKey::Ed25519(ed25519_key) => {
            verify_eddsa_signature(
                ed25519_key,
                signature_data.base.as_bytes(),
                &signature_data.signature,
            ).is_ok()
        },
        PublicKey::Rsa(rsa_key) => {
            verify_rsa_sha256_signature(
                rsa_key,
                signature_data.base.as_bytes(),
                &signature_data.signature,
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
            create_http_signature_cavage,
            HttpSigner,
        },
    };
    use super::*;

    #[test]
    fn test_parse_http_signature_cavage() {
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

        let signature_data = parse_http_signature_cavage(
            &request_method,
            &request_uri,
            &request_headers,
        ).unwrap();
        assert_eq!(signature_data.is_rfc9421, false);
        assert_eq!(
            signature_data.key_id.to_string(),
            "https://myserver.org/actor#main-key",
        );
        assert_eq!(
            signature_data.base,
            "(request-target): get /user/123/inbox\nhost: example.com\ndate: 20 Oct 2022 20:00:00 GMT",
        );
        assert_eq!(signature_data.signature, [181, 235, 45]);
        assert!(signature_data.expires_at < Utc::now());
        assert!(signature_data.content_digest.is_none());
    }

    #[test]
    fn test_parse_http_signature_rfc9421() {
        // https://datatracker.ietf.org/doc/html/rfc9421#name-signing-a-request-using-ed2
        let request_method = Method::POST;
        let request_uri = "/foo?param=Value&Pet=dog".parse::<Uri>().unwrap();
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            HeaderName::from_static("host"),
            HeaderValue::from_static("example.com"),
        );
        request_headers.insert(
            HeaderName::from_static("date"),
            HeaderValue::from_static("Tue, 20 Apr 2021 02:07:55 GMT"),
        );
        request_headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        request_headers.insert(
            HeaderName::from_static("content-digest"),
            HeaderValue::from_static("sha-512=:WZDPaVn/7XgHaAy8pmojAkGWoRx2UFChF41A2svX+TaPm+AbwAgBWnrIiYllu7BNNyealdVLvRwEmTHWXvJwew==:"),
        );
        request_headers.insert(
            HeaderName::from_static("content-length"),
            HeaderValue::from_static("18"),
        );
        // Key ID is different from the RFC test vector
        request_headers.insert(
            HeaderName::from_static("signature-input"),
            HeaderValue::from_static(r#"sig-b26=("date" "@method" "@path" "@authority" "content-type" "content-length");created=1618884473;keyid="https://example.com/actor#test-key-ed25519""#),
        );
        request_headers.insert(
            HeaderName::from_static("signature"),
            HeaderValue::from_static("sig-b26=:wqcAqbmYJ2ji2glfAMaRy4gruYYnx2nEFN2HN6jrnDnQCK1u02Gb04v9EDgwUPiu4A0w6vuQv5lIp5WPpBKRCw==:"),
        );
        let signature_data = parse_http_signature_rfc9421(
            &request_method,
            &request_uri,
            &request_headers,
        ).unwrap();

        let expected_signature_base =
r#""date": Tue, 20 Apr 2021 02:07:55 GMT
"@method": POST
"@path": /foo
"@authority": example.com
"content-type": application/json
"content-length": 18
"@signature-params": ("date" "@method" "@path" "@authority" "content-type" "content-length");created=1618884473;keyid="https://example.com/actor#test-key-ed25519""#;
        assert_eq!(signature_data.is_rfc9421, true);
        assert_eq!(signature_data.base, expected_signature_base);
        assert_eq!(signature_data.content_digest.is_some(), true);
    }

    #[test]
    fn test_create_and_verify_signature_cavage_get() {
        let request_method = Method::GET;
        let request_url = "https://example.org/inbox";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://myserver.org/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);
        let signed_headers = create_http_signature_cavage(
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
        let signature_data = parse_http_signature_cavage(
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
    fn test_create_and_verify_signature_cavage_post() {
        let request_method = Method::POST;
        let request_url = "https://example.org/inbox";
        let request_body = "{}";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://myserver.org/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);
        let signed_headers = create_http_signature_cavage(
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
        let signature_data = parse_http_signature_cavage(
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
    fn test_create_and_verify_signature_cavage_post_eddsa() {
        let request_method = Method::POST;
        let request_url = "https://server.example/inbox";
        let request_body = "{}";
        let signer_key = generate_weak_ed25519_key();
        let signer_key_id = "https://myserver.org/actor#ed25519-key".to_string();
        let signer = HttpSigner::new_ed25519(signer_key, signer_key_id);
        let signed_headers = create_http_signature_cavage(
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
        let signature_data = parse_http_signature_cavage(
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
