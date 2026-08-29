use form_urlencoded::Serializer;
use iri_string::{
    build::Builder,
    types::UriAbsoluteStr,
    validate::Error,
};

pub use iri_string::types::UriAbsoluteString;

pub fn append_to_redirect_uri(
    redirect_uri: &str,
    key: &str,
    value: &str,
) -> Result<String, Error> {
    let redirect_uri = UriAbsoluteString::try_from(redirect_uri)?;
    let query = redirect_uri
        .query_str()
        .unwrap_or_default()
        .to_owned();
    let query_new = Serializer::new(query)
        .append_pair(key, value)
        .finish();
    let mut builder = Builder::from(&redirect_uri);
    builder.query(&query_new);
    let output = builder.build::<UriAbsoluteStr>()?;
    Ok(output.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_to_redirect_uri() {
        let redirect_uri = "myapp://callback";
        let output = append_to_redirect_uri(redirect_uri, "code", "123").unwrap();
        assert_eq!(output, "myapp://callback?code=123");
    }

    #[test]
    fn test_append_to_redirect_uri_with_query() {
        let redirect_uri = "https://social.example?test=123";
        let output = append_to_redirect_uri(redirect_uri, "code", "123").unwrap();
        assert_eq!(output, "https://social.example?test=123&code=123");
    }
}
