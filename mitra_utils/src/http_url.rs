use std::fmt;
use std::str::FromStr;

use oxiri::Iri;
use url::Url;

pub struct HttpUrl(Iri<String>);

impl HttpUrl {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        // RFC-3987
        let iri = Iri::parse(value).map_err(|_| "invalid IRI")?;
        // Verify scheme
        match iri.scheme() {
            "http" | "https" => (),
            _ => return Err("invalid URL scheme"),
        };
        if iri.authority().unwrap_or_default() == "" {
            return Err("invalid URL authority");
        };
        // Additional validation (WHATWG URL spec)
        let url = Url::parse(value).map_err(|_| "invalid URL")?;
        url.host().ok_or("invalid URL")?;
        let http_url = Self(iri.into());
        Ok(http_url)
    }

    fn scheme(&self) -> &str {
        self.0.scheme()
    }

    fn authority(&self) -> &str {
        self.0.authority().expect("authority should be present")
    }

    pub fn path(&self) -> &str {
        self.0.path()
    }

    pub fn query(&self) -> Option<&str> {
        self.0.query()
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
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_url() {
        let url = "https://social.example/users?user_id=123#main-key";
        let http_url = HttpUrl::parse(url).unwrap();
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
        assert_eq!(http_url.to_string(), url);
    }

    #[test]
    fn test_http_url_idn() {
        let url = "https://räksmörgås.josefsson.org/raksmorgas.jpg";
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
        assert_eq!(error, "invalid IRI");
    }
}
