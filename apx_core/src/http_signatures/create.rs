//! Create HTTP signatures
use chrono::Utc;
use http::Method;
use url::{Url, ParseError as UrlError};

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
    http_digest::{create_digest_header, ContentDigest},
};

const HTTP_SIGNATURE_ALGORITHM: &str = "rsa-sha256";
const HTTP_SIGNATURE_ALGORITHM_HS2019: &str = "hs2019";
// https://www.rfc-editor.org/rfc/rfc9110#http.date
const HTTP_SIGNATURE_DATE_FORMAT: &str = "%a, %d %b %Y %T GMT";

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

pub struct HttpSignatureHeaders {
    pub host: String,
    pub date: String,
    pub digest: Option<String>,
    pub signature: String,
}

#[derive(thiserror::Error, Debug)]
pub enum HttpSignatureError {
    #[error("invalid request url")]
    UrlError(#[from] UrlError),

    #[error("signing error")]
    SigningError(#[from] RsaError),
}

/// Creates HTTP signature according to the old HTTP Signatures Spec
/// <https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures>
pub fn create_http_signature_cavage(
    request_method: Method,
    request_url: &str,
    request_body: &[u8],
    signer: &HttpSigner,
) -> Result<HttpSignatureHeaders, HttpSignatureError> {
    let request_url_object = Url::parse(request_url)?;
    let request_target = format!(
        "{} {}",
        request_method.as_str().to_lowercase(),
        request_url_object.path(),
    );
    // TODO: Host header may contain port
    let host = request_url_object.host_str()
        .ok_or(UrlError::EmptyHost)?
        .to_string();
    let date = Utc::now().format(HTTP_SIGNATURE_DATE_FORMAT).to_string();
    let maybe_digest_header = if request_body.is_empty() {
        None
    } else {
        let digest = ContentDigest::new(request_body);
        let digest_header = create_digest_header(&digest);
        Some(digest_header)
    };

    let mut headers = vec![
        ("(request-target)", &request_target),
        ("host", &host),
        ("date", &date),
    ];
    if let Some(digest_header) = maybe_digest_header.as_ref() {
        headers.push(("digest", digest_header));
    };

    let message = headers.iter()
        .map(|(name, value)| format!("{}: {}", name, value))
        .collect::<Vec<String>>()
        .join("\n");
    let headers_parameter = headers.iter()
        .map(|(name, _)| name.to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let (signature, algorithm) = match signer.key {
        SecretKey::Ed25519(ref secret_key) => {
            let signature =
                create_eddsa_signature(secret_key, message.as_bytes()).to_vec();
            (signature, HTTP_SIGNATURE_ALGORITHM_HS2019)
        },
        SecretKey::Rsa(ref secret_key) => {
            let signature =
                create_rsa_sha256_signature(secret_key, message.as_bytes())?;
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

#[cfg(test)]
mod tests {
    use crate::crypto_rsa::generate_weak_rsa_key;
    use super::*;

    #[test]
    fn test_create_signature_cavage_get() {
        let request_url = "https://example.org/inbox";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://myserver.org/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);

        let headers = create_http_signature_cavage(
            Method::GET,
            request_url,
            b"",
            &signer,
        ).unwrap();

        assert_eq!(headers.host, "example.org");
        assert_eq!(headers.digest, None);
        let expected_signature_header = concat!(
            r#"keyId="https://myserver.org/actor#main-key","#,
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
    fn test_create_signature_cavage_post() {
        let request_url = "https://example.org/inbox";
        let request_body = "{}";
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://myserver.org/actor#main-key".to_string();
        let signer = HttpSigner::new_rsa(signer_key, signer_key_id);

        let result = create_http_signature_cavage(
            Method::POST,
            request_url,
            request_body.as_bytes(),
            &signer,
        );
        assert_eq!(result.is_ok(), true);

        let headers = result.unwrap();
        assert_eq!(headers.host, "example.org");
        assert_eq!(
            headers.digest.unwrap(),
            "SHA-256=RBNvo1WzZ4oRRq0W9+hknpT7T8If536DEMBg9hyq/4o=",
        );
        let expected_signature_header = concat!(
            r#"keyId="https://myserver.org/actor#main-key","#,
            r#"algorithm="rsa-sha256","#,
            r#"headers="(request-target) host date digest","#,
            r#"signature=""#,
        );
        assert_eq!(
            headers.signature.starts_with(expected_signature_header),
            true,
        );
    }
}
