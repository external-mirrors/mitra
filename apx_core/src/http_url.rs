use std::fmt;
use std::str::FromStr;

use iri_string::types::UriString;
use url::Url;

use crate::url::common::Origin;

pub struct Hostname(String);

impl Hostname {
    fn new_unchecked(value: &str) -> Self {
        Self(value.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Hostname {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.as_str())
    }
}

pub fn parse_http_url_whatwg(url: &str) -> Result<Url, &'static str> {
    let url = Url::parse(url).map_err(|_| "invalid URL")?;
    match url.scheme() {
        "http" | "https" => (),
        _ => return Err("invalid URL scheme"),
    };
    url.host().ok_or("invalid HTTP URL")?;
    Ok(url)
}

fn parse_port_number(port: &str) -> Result<u16, &'static str> {
    u16::from_str(port).map_err(|_| "invalid port number")
}

/// Valid HTTP(S) URI (RFC-3986)
#[derive(Clone)]
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
        // Validate URI
        if uri.authority_str().unwrap_or_default() == "" {
            return Err("invalid URL authority");
        };
        let authority_components = uri.authority_components()
            .ok_or("invalid URL authority")?;
        authority_components.port()
            .map(parse_port_number)
            .transpose()?;
        // Additional validation (WHATWG URL spec)
        parse_http_url_whatwg(value)?;
        let http_url = Self(uri);
        Ok(http_url)
    }

    fn scheme(&self) -> &str {
        self.0.scheme_str()
    }

    pub fn authority(&self) -> &str {
        self.0.authority_str().expect("authority should be present")
    }

    pub fn path(&self) -> &str {
        self.0.path_str()
    }

    pub fn query(&self) -> Option<&str> {
        self.0.query_str()
    }

    fn fragment(&self) -> Option<&str> {
        self.0.fragment_str()
    }

    pub fn base(&self) -> String {
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

    pub fn hostname(&self) -> Hostname {
        let authority_components = self.0.authority_components()
            .expect("authority should be present");
        let hostname = authority_components.host();
        if hostname.starts_with('[') && hostname.ends_with(']') {
            Hostname::new_unchecked(&hostname[1 .. hostname.len() - 1])
        } else {
            Hostname::new_unchecked(hostname)
        }
    }

    // https://www.rfc-editor.org/rfc/rfc6454.html
    pub fn origin(&self) -> Origin {
        let authority_components = self.0.authority_components()
            .expect("authority should be present");
        let host = authority_components.host();
        let port = authority_components.port()
            .map(parse_port_number)
            .transpose()
            .expect("port number should be valid")
            .unwrap_or_else(|| match self.scheme() {
                "http" => 80,
                "https" => 443,
                _ => panic!("scheme should be valid"),
            });
        Origin::new(self.scheme(), host, port)
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
    fn test_http_url_ipv4_address() {
        let url = "http://10.4.1.13/test";
        let http_url = HttpUrl::parse(url).unwrap();
        assert_eq!(http_url.authority(), "10.4.1.13");
    }

    #[test]
    fn test_http_url_ipv6_address() {
        let url = "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/test";
        let http_url = HttpUrl::parse(url).unwrap();
        assert_eq!(http_url.authority(), "[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]");
    }

    #[test]
    fn test_http_url_invalid_ipv6_address() {
        let url = "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be/test";
        let error = HttpUrl::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI");
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
    fn test_http_url_invalid_port() {
        let url = "https://social.example:9999999/test";
        let error = HttpUrl::parse(url).err().unwrap();
        assert_eq!(error, "invalid port number");
    }

    #[test]
    fn test_http_url_hostname() {
        let http_url = HttpUrl::parse("https://social.example/test").unwrap();
        assert_eq!(http_url.hostname().as_str(), "social.example");

        let http_url = HttpUrl::parse("http://127.0.0.1:8380/test").unwrap();
        assert_eq!(http_url.hostname().as_str(), "127.0.0.1");

        let http_url = HttpUrl::parse("http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/test").unwrap();
        assert_eq!(http_url.hostname().as_str(), "319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be");
    }

    #[test]
    fn test_origin() {
        let http_url = HttpUrl::parse("https://social.example/test").unwrap();
        let origin = http_url.origin();
        assert_eq!(origin, Origin::new("https", "social.example", 443));

        let http_url = HttpUrl::parse("http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/test").unwrap();
        let origin = http_url.origin();
        assert_eq!(origin, Origin::new("http", "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion", 80));

        let http_url = HttpUrl::parse("http://127.0.0.1:8380/test").unwrap();
        let origin = http_url.origin();
        assert_eq!(origin, Origin::new("http", "127.0.0.1", 8380));
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
