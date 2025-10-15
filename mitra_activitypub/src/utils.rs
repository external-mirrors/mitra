use apx_core::{
    url::{
        ap_uri::{is_ap_uri, ApUri},
        canonical::{with_gateway, CanonicalUri},
        http_uri::{normalize_http_url, HttpUri},
    },
};

use mitra_models::database::{DatabaseError, DatabaseTypeError};

// TODO: validation should happen during actor data deserialization

// URLs stored in database are not guaranteed to be valid
// according to HttpUri::parse.
// This function normalizes URL before parsing to avoid errors.
// WARNING: Adds a trailing slash if path is empty.
pub fn parse_http_url_from_db(
    url: &str,
) -> Result<HttpUri, DatabaseTypeError> {
    let normalized_url = normalize_http_url(url)
        .map_err(|_| DatabaseTypeError)?;
    let http_uri = HttpUri::parse(&normalized_url)
        .map_err(|_| DatabaseTypeError)?;
    Ok(http_uri)
}

pub fn parse_id_from_db(
    url: &str,
) -> Result<CanonicalUri, DatabaseError> {
    // WARNING: returns error if stored URI is not canonical
    // WARNING: returns error if stored HTTP URI is not valid
    let canonical_uri = CanonicalUri::parse_canonical(url)
        .map_err(|_| DatabaseTypeError)?;
    Ok(canonical_uri)
}

// URLs associated with portable actors in database
// are not guaranteed to be 'ap' URIs. They could be HTTP URLs.
pub fn db_url_to_http_url(
    url: &str,
    gateway: &str,
) -> Result<String, DatabaseTypeError> {
    let http_url = if is_ap_uri(url) {
        let ap_uri = ApUri::parse(url).map_err(|_| DatabaseTypeError)?;
        with_gateway(&ap_uri, gateway)
    } else {
        url.to_owned()
    };
    Ok(http_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_url_from_db_uppercase_host() {
        let url = "HTTPS://EXAMPLE.SOCIAL/actor";
        let output = parse_http_url_from_db(url).unwrap();
        assert_eq!(
            output.as_str(),
            "https://example.social/actor",
        );
    }

    #[test]
    fn test_parse_http_url_from_db_unicode() {
        let url = "https://zh.wikipedia.org/wiki/百分号编码";
        let output = parse_http_url_from_db(url).unwrap();
        assert_eq!(
            output.as_str(),
            "https://zh.wikipedia.org/wiki/%E7%99%BE%E5%88%86%E5%8F%B7%E7%BC%96%E7%A0%81",
        );
    }
}
