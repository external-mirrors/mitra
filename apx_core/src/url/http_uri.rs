//! HTTP(S) URIs
use std::fmt;
use std::str::FromStr;

use iri_string::types::UriString;
use serde::{
    de::{Error as DeserializerError},
    Deserialize,
    Deserializer,
};
use url::Url;

use crate::url::common::Origin;

#[derive(PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct HttpUri(UriString);

impl HttpUri {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let uri = UriString::from_str(value).map_err(|_| "invalid URI")?;
        // TODO: accept only normalized URIs
        // Verify scheme
        match uri.scheme_str() {
            "http" | "https" => (),
            _ => return Err("invalid URI scheme"),
        };
        // Validate URI
        if uri.authority_str().unwrap_or_default() == "" {
            return Err("invalid URI authority");
        };
        let authority_components = uri.authority_components()
            .ok_or("invalid URI authority")?;
        if authority_components.host().to_lowercase() !=
            authority_components.host()
        {
            return Err("invalid URI host");
        };
        authority_components.port()
            .map(parse_port_number)
            .transpose()?;
        // Additional validation (WHATWG URL spec)
        parse_http_url_whatwg(value)?;
        let http_uri = Self(uri);
        Ok(http_uri)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    /// Returns URI scheme
    pub fn scheme(&self) -> &str {
        self.0.scheme_str()
    }

    pub(crate) fn host(&self) -> &str {
        let authority_components = self.0.authority_components()
            .expect("authority should be present");
        authority_components.host()
    }

    pub(crate) fn port(&self) -> Option<u16> {
        let authority_components = self.0.authority_components()
            .expect("authority should be present");
        authority_components.port()
            .map(parse_port_number)
            .transpose()
            .expect("port number should be valid")
    }

    fn port_or_known_default(&self) -> u16 {
        self.port()
            .unwrap_or_else(|| match self.scheme() {
                "http" => 80,
                "https" => 443,
                _ => panic!("scheme should be valid"),
            })
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

    /// Returns host name of this URI
    pub fn hostname(&self) -> Hostname {
        // Similar to urls::get_hostname
        Hostname::new_unchecked(self.host())
    }

    /// Returns origin of this URI
    ///
    /// <https://www.rfc-editor.org/rfc/rfc6454.html>
    pub fn origin(&self) -> Origin {
        Origin::new_tuple(
            self.scheme(),
            self.host(),
            self.port_or_known_default(),
        )
    }
}

/// Returns the original URI string
impl fmt::Display for HttpUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl FromStr for HttpUri {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl<'de> Deserialize<'de> for HttpUri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let value: String = Deserialize::deserialize(deserializer)?;
        Self::parse(&value).map_err(DeserializerError::custom)
    }
}

pub fn normalize_http_url(url: &str) -> Result<String, &'static str> {
    // WHATWG URL spec
    // See also: https://www.rfc-editor.org/rfc/rfc3986#section-6.2.3
    // WARNING: Adds a trailing slash
    let url = parse_http_url_whatwg(url)?;
    Ok(url.to_string())
}

pub fn is_same_http_origin(
    uri_1: &str,
    uri_2: &str,
) -> Result<bool, &'static str> {
    let uri_1 = HttpUri::parse(uri_1)?;
    let uri_2 = HttpUri::parse(uri_2)?;
    let is_same = uri_1.origin() == uri_2.origin();
    Ok(is_same)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let url = "https://social.example/users?user_id=123#main-key";
        let http_uri = HttpUri::parse(url).unwrap();
        assert_eq!(http_uri.to_relative(), "/users?user_id=123#main-key");
        assert_eq!(http_uri.as_str(), url);
        assert_eq!(http_uri.to_string(), url);
    }

    #[test]
    fn test_parse_with_ipv4_address() {
        let url = "http://10.4.1.13/test";
        let http_uri = HttpUri::parse(url).unwrap();
        assert_eq!(http_uri.authority(), "10.4.1.13");
    }

    #[test]
    fn test_parse_with_ipv6_address() {
        let url = "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/test";
        let http_uri = HttpUri::parse(url).unwrap();
        assert_eq!(http_uri.authority(), "[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]");
    }

    #[test]
    fn test_parse_with_invalid_ipv6_address() {
        let url = "http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be/test";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI");
    }

    #[test]
    fn test_parse_onion() {
        let url = "http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/users/alice";
        let http_uri = HttpUri::parse(url).unwrap();
        assert_eq!(http_uri.to_string(), url);
    }

    #[test]
    fn test_parse_no_path() {
        let url = "https://social.example";
        let http_uri = HttpUri::parse(url).unwrap();
        assert_eq!(http_uri.path(), "");
        assert_eq!(http_uri.to_relative(), "");
        assert_eq!(http_uri.to_string(), url);
    }

    #[test]
    fn test_parse_idn() {
        let url = "https://räksmörgås.josefsson.org/raksmorgas.jpg";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI");
    }

    #[test]
    fn test_parse_path_percent_encoded() {
        let url = "https://bridge.example/actors/https%3A%2F%2Fthreads%2Enet%2Fap%2Fusers%2F17841400033000000%2F";
        let http_uri = HttpUri::parse(url).unwrap();
        assert_eq!(http_uri.to_string(), url);
    }

    #[test]
    fn test_parse_scheme_uppercase() {
        let url = "HTTP://social.example/users/alice";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI scheme");
    }

    #[test]
    fn test_parse_host_uppercase() {
        let url = "https://Social.Example/users/alice";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI host");
    }

    #[test]
    fn test_parse_ftp_scheme() {
        let url = "ftp://ftp.social.example/";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI scheme");
    }

    #[test]
    fn test_parse_no_authority() {
        let url = "http:///home/User/2ndFile.html";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI authority");
    }

    #[test]
    fn test_parse_with_whitespace() {
        let url = "https://rebased.taihou.website/emoji/taihou.website emojos/nix.png";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid URI");
    }

    #[test]
    fn test_parse_invalid_port() {
        let url = "https://social.example:9999999/test";
        let error = HttpUri::parse(url).err().unwrap();
        assert_eq!(error, "invalid port number");
    }

    #[test]
    fn test_hostname() {
        let http_uri = HttpUri::parse("https://social.example/test").unwrap();
        assert_eq!(http_uri.hostname().as_str(), "social.example");

        let http_uri = HttpUri::parse("http://127.0.0.1:8380/test").unwrap();
        assert_eq!(http_uri.hostname().as_str(), "127.0.0.1");

        let http_uri = HttpUri::parse("http://[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]/test").unwrap();
        assert_eq!(http_uri.hostname().as_str(), "[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]");
    }

    #[test]
    fn test_origin() {
        let http_uri = HttpUri::parse("https://social.example/test").unwrap();
        let origin = http_uri.origin();
        assert_eq!(origin, Origin::new_tuple("https", "social.example", 443));

        let http_uri = HttpUri::parse("http://2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion/test").unwrap();
        let origin = http_uri.origin();
        assert_eq!(origin, Origin::new_tuple("http", "2gzyxa5ihm7nsggfxnu52rck2vv4rvmdlkiu3zzui5du4xyclen53wid.onion", 80));

        let http_uri = HttpUri::parse("http://127.0.0.1:8380/test").unwrap();
        let origin = http_uri.origin();
        assert_eq!(origin, Origin::new_tuple("http", "127.0.0.1", 8380));
    }

    #[test]
    fn test_normalize_http_url_no_path() {
        let url = "https://social.example";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://social.example/");
        assert!(HttpUri::parse(&output).is_ok());
    }

    #[test]
    fn test_normalize_http_url_idn() {
        let url = "https://räksmörgås.josefsson.org/raksmorgas.jpg";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://xn--rksmrgs-5wao1o.josefsson.org/raksmorgas.jpg");
        assert!(HttpUri::parse(&output).is_ok());
    }

    #[test]
    fn test_normalize_http_url_with_whitespace() {
        let url = "https://social.example/path with a space/1";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://social.example/path%20with%20a%20space/1");
        assert!(HttpUri::parse(&output).is_ok());
    }

    #[test]
    fn test_normalize_http_url_unicode() {
        let url = "https://zh.wikipedia.org/wiki/百分号编码";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://zh.wikipedia.org/wiki/%E7%99%BE%E5%88%86%E5%8F%B7%E7%BC%96%E7%A0%81");
        assert!(HttpUri::parse(&output).is_ok());
    }

    #[test]
    fn test_normalize_http_url_uppercase() {
        let url = "Https://Social.Example/Path";
        let output = normalize_http_url(url).unwrap();
        assert_eq!(output, "https://social.example/Path");
        assert!(HttpUri::parse(&output).is_ok());
    }
}
