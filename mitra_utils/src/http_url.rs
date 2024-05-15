use std::fmt;
use std::str::FromStr;

use iri_string::types::UriString;
use url::Url;

pub(crate) fn parse_http_url_whatwg(url: &str) -> Result<Url, &'static str> {
    let url = Url::parse(url).map_err(|_| "invalid URL")?;
    match url.scheme() {
        "http" | "https" => (),
        _ => return Err("invalid URL scheme"),
    };
    url.host().ok_or("invalid HTTP URL")?;
    Ok(url)
}

/// Valid HTTP(S) URI (RFC-3986)
pub struct HttpUrl(UriString);

impl HttpUrl {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let uri = UriString::from_str(value).map_err(|_| "invalid URI")?;
        // TODO: accept only normalized URIs
        // Verify scheme
        match uri.scheme_str() {
            "http" | "https" => (),
            _ => return Err("invalid URL scheme"),
        };
        if uri.authority_str().unwrap_or_default() == "" {
            return Err("invalid URL authority");
        };
        // Additional validation (WHATWG URL spec)
        parse_http_url_whatwg(value)?;
        let http_url = Self(uri);
        Ok(http_url)
    }

    fn scheme(&self) -> &str {
        self.0.scheme_str()
    }

    fn authority(&self) -> &str {
        self.0.authority_str().expect("authority should be present")
    }

    pub fn path(&self) -> &str {
        self.0.path_str()
    }

    pub fn query(&self) -> Option<&str> {
        self.0.query_str()
    }

    fn fragment(&self) -> Option<&str> {
        self.0.fragment().map(|fragment| fragment.as_str())
    }

    pub fn origin(&self) -> String {
        format!(
            "{}://{}",
            self.scheme(),
            self.authority(),
        )
    }

    pub fn without_query_and_fragment(&self) -> String {
        format!(
            "{}://{}{}",
            self.scheme(),
            self.authority(),
            self.path(),
        )
    }

    pub fn without_fragment(&self) -> String {
        format!(
            "{}{}",
            self.without_query_and_fragment(),
            self.query().map(|query| format!("?{query}")).unwrap_or_default(),
        )
    }

    pub fn to_relative(&self) -> String {
        format!(
            "{}{}{}",
            self.path(),
            self.query().map(|query| format!("?{query}")).unwrap_or_default(),
            self.fragment().map(|frag| format!("#{frag}")).unwrap_or_default(),
        )
    }
}

/// Returns the original URI string
impl fmt::Display for HttpUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl FromStr for HttpUrl {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

pub fn normalize_http_url(url: &str) -> Result<String, &'static str> {
    // WHATWG URL spec
    // See also: https://www.rfc-editor.org/rfc/rfc3986#section-6.2.3
    // WARNING: Adds a trailing slash
    let url = parse_http_url_whatwg(url)?;
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_url() {
        let url = "https://social.example/users?user_id=123#main-key";
        let http_url = HttpUrl::parse(url).unwrap();
        assert_eq!(http_url.to_relative(), "/users?user_id=123#main-key");
        assert_eq!(http_url.to_string(), url);
    }

    #[test]
    fn test_http_url_onion() {
        let url = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/users/alice";
        let http_url = HttpUrl::parse(url).unwrap();
        assert_eq!(http_url.to_string(), url);
    }

    #[test]
    fn test_http_url_no_path() {
        let url = "https://social.example";
        let http_url = HttpUrl::parse(url).unwrap();
        assert_eq!(http_url.path(), "");
        assert_eq!(http_url.to_relative(), "");
        assert_eq!(http_url.to_string(), url);
    }

    #[test]
    fn test_http_url_idn() {
        let url = "https://räksmörgås.josefsson.org/raksmorgas.jpg";
        let error = HttpUrl::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI");
    }

    #[test]
    fn test_http_url_percent_encoded() {
        let url = "https://bridge.example/actors/https%3A%2F%2Fthreads%2Enet%2Fap%2Fusers%2F17841400033000000%2F";
        let http_url = HttpUrl::parse(url).unwrap();
        assert_eq!(http_url.to_string(), url);
    }

    #[test]
    fn test_http_url_scheme_uppercase() {
        let url = "HTTP://social.example/users/alice";
        let error = HttpUrl::parse(url).err().unwrap();
        assert_eq!(error, "invalid URL scheme");
    }

    #[test]
    fn test_http_url_ftp() {
        let url = "ftp://ftp.social.example/";
        let error = HttpUrl::parse(url).err().unwrap();
        assert_eq!(error, "invalid URL scheme");
    }

    #[test]
    fn test_http_url_no_authority() {
        let url = "http:///home/User/2ndFile.html";
        let error = HttpUrl::parse(url).err().unwrap();
        assert_eq!(error, "invalid URL authority");
    }

    #[test]
    fn test_http_url_with_whitespace() {
        let url = "https://rebased.taihou.website/emoji/taihou.website emojos/nix.png";
        let error = HttpUrl::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI");
    }

    #[test]
    fn test_normalize_http_url_no_path() {
        let url = "https://social.example";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://social.example/");
        assert!(HttpUrl::parse(&output).is_ok());
    }

    #[test]
    fn test_normalize_http_url_idn() {
        let url = "https://räksmörgås.josefsson.org/raksmorgas.jpg";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://xn--rksmrgs-5wao1o.josefsson.org/raksmorgas.jpg");
        assert!(HttpUrl::parse(&output).is_ok());
    }

    #[test]
    fn test_normalize_http_url_with_whitespace() {
        let url = "https://social.example/path with a space/1";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://social.example/path%20with%20a%20space/1");
        assert!(HttpUrl::parse(&output).is_ok());
    }

    #[test]
    fn test_normalize_http_url_unicode() {
        let url = "https://zh.wikipedia.org/wiki/百分号编码";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://zh.wikipedia.org/wiki/%E7%99%BE%E5%88%86%E5%8F%B7%E7%BC%96%E7%A0%81");
        assert!(HttpUrl::parse(&output).is_ok());
    }
}
