//! Create HTTP signatures
use chrono::Utc;
use http::Method;
use sfv::{
    BareItem,
    Dictionary,
    InnerList,
    Item,
    ListEntry,
    Parameters,
    SerializeValue,
};
use thiserror::Error;

use crate::{
    base64,
    crypto::common::SecretKey,
    crypto_eddsa::{
        create_eddsa_signature,
        Ed25519SecretKey,
    },
    crypto_rsa::{
        create_rsa_sha256_signature,
        RsaError,
        RsaSecretKey,
    },
    http_digest::{
        create_content_digest_header,
        create_digest_header,
        ContentDigest,
    },
    url::http_uri::{normalize_http_url, HttpUri},
};

const HTTP_SIGNATURE_ALGORITHM: &str = "rsa-sha256";
const HTTP_SIGNATURE_ALGORITHM_HS2019: &str = "hs2019";
// https://www.rfc-editor.org/rfc/rfc9110#http.date
const HTTP_SIGNATURE_DATE_FORMAT: &str = "%a, %d %b %Y %T GMT";

const RFC9421_SIGNATURE_LABEL: &str = "sig1";

/// Entity that creates an HTTP signature
pub struct HttpSigner {
    pub key: SecretKey,
    pub key_id: String,
}

impl HttpSigner {
    pub fn new_rsa(key: RsaSecretKey, key_id: String) -> Self {
        Self {
            key: SecretKey::Rsa(key),
            key_id,
        }
    }

    pub fn new_ed25519(key: Ed25519SecretKey, key_id: String) -> Self {
        Self {
            key: SecretKey::Ed25519(key),
            key_id,
        }
    }
}

/// HTTP headers for signed request (Draft-Cavage)
pub struct HttpSignatureHeaders {
    pub host: String,
    pub date: String,
    pub digest: Option<String>,
    pub signature: String,
}

/// Errors that may occur during signature generation
#[derive(Debug, Error)]
pub enum HttpSignatureError {
    #[error("invalid request URL: {0}")]
    UrlError(&'static str),

    #[error("serialization error")]
    SerializationError,

    #[error("signing error")]
    SigningError(#[from] RsaError),
}

/// Creates HTTP signature according to the old HTTP Signatures Spec  
/// <https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures>
pub fn create_http_signature_cavage(
    request_method: Method,
    request_url: &str,
    maybe_request_body: Option<&[u8]>,
    signer: &HttpSigner,
) -> Result<HttpSignatureHeaders, HttpSignatureError> {
    // URL is normalized
    let request_url = normalize_http_url(request_url)
        .map_err(HttpSignatureError::UrlError)?;
    let request_uri = HttpUri::parse(&request_url)
        .map_err(HttpSignatureError::UrlError)?;
    let request_target = format!(
        "{} {}",
        request_method.as_str().to_lowercase(),
        request_uri.path(),
    );
    let host = if let Some(port) = request_uri.port() {
        format!("{}:{}", request_uri.host(), port)
    } else {
        request_uri.host().to_owned()
    };
    let date = Utc::now().format(HTTP_SIGNATURE_DATE_FORMAT).to_string();
    let maybe_digest_header = if let Some(body) = maybe_request_body {
        let digest = ContentDigest::new(body);
        let digest_header = create_digest_header(&digest);
        Some(digest_header)
    } else {
        None
    };

    let mut headers = vec![
        ("(request-target)", &request_target),
        ("host", &host),
        ("date", &date),
    ];
    if let Some(digest_header) = maybe_digest_header.as_ref() {
        headers.push(("digest", digest_header));
    };

    let signature_base = headers.iter()
        .map(|(name, value)| format!("{}: {}", name, value))
        .collect::<Vec<String>>()
        .join("\n");
    let headers_parameter = headers.iter()
        .map(|(name, _)| name.to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let (signature, algorithm) = match signer.key {
        SecretKey::Ed25519(ref secret_key) => {
            let signature = create_eddsa_signature(
                secret_key,
                signature_base.as_bytes(),
            );
            (signature.to_vec(), HTTP_SIGNATURE_ALGORITHM_HS2019)
        },
        SecretKey::Rsa(ref secret_key) => {
            let signature = create_rsa_sha256_signature(
                secret_key,
                signature_base.as_bytes(),
            )?;
            (signature, HTTP_SIGNATURE_ALGORITHM)
        },
    };
    let signature_parameter = base64::encode(signature);
    let signature_header = format!(
        r#"keyId="{}",algorithm="{}",headers="{}",signature="{}""#,
        signer.key_id,
        algorithm,
        headers_parameter,
        signature_parameter,
    );
    let headers = HttpSignatureHeaders {
        host,
        date,
        digest: maybe_digest_header,
        signature: signature_header,
    };
    Ok(headers)
}

/// HTTP headers for signed request (RFC-9421)
pub struct HttpSignatureHeadersRfc9421 {
    pub content_digest: Option<String>,
    pub signature: String,
    pub signature_input: String,
}

/// Creates RFC-9421 HTTP message signature  
/// <https://datatracker.ietf.org/doc/html/rfc9421>
pub fn create_http_signature_rfc9421(
    request_method: Method,
    request_url: &str,
    maybe_request_body: Option<&[u8]>,
    signer: &HttpSigner,
) -> Result<HttpSignatureHeadersRfc9421, HttpSignatureError> {
    let request_url = normalize_http_url(request_url)
        .map_err(HttpSignatureError::UrlError)?;
    let request_uri = HttpUri::parse(&request_url)
        .map_err(HttpSignatureError::UrlError)?;
    let created = Utc::now().timestamp();
    let maybe_content_digest_header = if let Some(body) = maybe_request_body {
        let digest = ContentDigest::new(body);
        let digest_header = create_content_digest_header(&digest)
            .map_err(|_| HttpSignatureError::SerializationError)?;
        Some(digest_header)
    } else {
        None
    };

    // Prepare signature input
    let mut signature_base_entries = vec![
        ("@method", request_method.to_string()),
        ("@target-uri", request_uri.to_string()),
    ];
    if let Some(ref digest_header) = maybe_content_digest_header {
        signature_base_entries.push(("content-digest", digest_header.clone()));
    };

    let component_list_items = signature_base_entries.iter()
        .map(|(name, _)| Item::new(BareItem::String(name.to_string())))
        .collect();
    let mut parameters = Parameters::new();
    parameters.insert("keyid".to_owned(), BareItem::String(signer.key_id.clone()));
    parameters.insert("created".to_owned(), BareItem::Integer(created));
    let signature_param_list =
        InnerList::with_params(component_list_items, parameters);
    let signature_params = vec![ListEntry::InnerList(signature_param_list.clone())]
        .serialize_value()
        .map_err(|_| HttpSignatureError::SerializationError)?;
    signature_base_entries.push(("@signature-params", signature_params));

    // Create signature
    let signature_base = signature_base_entries
        .into_iter()
        .map(|(id, value)| format!(r#""{id}": {value}"#))
        .collect::<Vec<_>>()
        .join("\n");
    let (signature, _) = match signer.key {
        SecretKey::Ed25519(ref secret_key) => {
            let signature =
                create_eddsa_signature(secret_key, signature_base.as_bytes()).to_vec();
            (signature, HTTP_SIGNATURE_ALGORITHM_HS2019)
        },
        SecretKey::Rsa(ref secret_key) => {
            let signature =
                create_rsa_sha256_signature(secret_key, signature_base.as_bytes())?;
            (signature, HTTP_SIGNATURE_ALGORITHM)
        },
    };

    // Create Signature-Input header
    let mut signature_input_dict = Dictionary::new();
    signature_input_dict.insert(
        RFC9421_SIGNATURE_LABEL.to_owned(),
        ListEntry::InnerList(signature_param_list),
    );
    let signature_input_header = signature_input_dict.serialize_value()
        .map_err(|_| HttpSignatureError::SerializationError)?;
    // Create Signature header
    let mut signature_dict = Dictionary::new();
    signature_dict.insert(
        RFC9421_SIGNATURE_LABEL.to_owned(),
        ListEntry::Item(Item::new(BareItem::ByteSeq(signature))),
    );
    let signature_header = signature_dict.serialize_value()
        .map_err(|_| HttpSignatureError::SerializationError)?;

    let headers = HttpSignatureHeadersRfc9421 {
        content_digest: maybe_content_digest_header,
        signature: signature_header,
        signature_input: signature_input_header,
    };
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use crate::crypto_rsa::generate_weak_rsa_key;
    use super::*;

    #[test]
    fn test_create_http_signature_cavage_get() {
        let request_url = "https://verifier.example/private-object";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://signer.example/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);

        let headers = create_http_signature_cavage(
            Method::GET,
            request_url,
            None,
            &signer,
        ).unwrap();

        assert_eq!(headers.host, "verifier.example");
        assert_eq!(headers.digest, None);
        let expected_signature_header = concat!(
            r#"keyId="https://signer.example/actor#main-key","#,
            r#"algorithm="rsa-sha256","#,
            r#"headers="(request-target) host date","#,
            r#"signature=""#,
        );
        assert_eq!(
            headers.signature.starts_with(expected_signature_header),
            true,
        );
    }

    #[test]
    fn test_create_http_signature_cavage_get_with_port() {
        let request_url = "http://127.0.0.1:1234/private-object";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK#z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);
        let headers = create_http_signature_cavage(
            Method::GET,
            request_url,
            None,
            &signer,
        ).unwrap();
        assert_eq!(headers.host, "127.0.0.1:1234");
    }

    #[test]
    fn test_create_http_signature_cavage_post() {
        let request_url = "https://verifier.example/inbox";
        let request_body = "{}";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://signer.example/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);

        let result = create_http_signature_cavage(
            Method::POST,
            request_url,
            Some(request_body.as_bytes()),
            &signer,
        );
        assert_eq!(result.is_ok(), true);

        let headers = result.unwrap();
        assert_eq!(headers.host, "verifier.example");
        assert_eq!(
            headers.digest.unwrap(),
            "SHA-256=RBNvo1WzZ4oRRq0W9+hknpT7T8If536DEMBg9hyq/4o=",
        );
        let expected_signature_header = concat!(
            r#"keyId="https://signer.example/actor#main-key","#,
            r#"algorithm="rsa-sha256","#,
            r#"headers="(request-target) host date digest","#,
            r#"signature=""#,
        );
        assert_eq!(
            headers.signature.starts_with(expected_signature_header),
            true,
        );
    }

    #[test]
    fn test_create_http_signature_rfc9421_get() {
        let request_url = "https://verifier.example/private-object";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://signer.example/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);

        let headers = create_http_signature_rfc9421(
            Method::GET,
            request_url,
            None,
            &signer,
        ).unwrap();
        assert_eq!(headers.content_digest, None);
    }

    #[test]
    fn test_create_http_signature_rfc9421_post() {
        let request_url = "https://verifier.example/inbox";
        let request_body = "{}";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://signer.example/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);

        let headers = create_http_signature_rfc9421(
            Method::POST,
            request_url,
            Some(request_body.as_bytes()),
            &signer,
        ).unwrap();
        assert_eq!(
            headers.content_digest.unwrap(),
            "sha-256=:RBNvo1WzZ4oRRq0W9+hknpT7T8If536DEMBg9hyq/4o=:",
        );
    }
}
