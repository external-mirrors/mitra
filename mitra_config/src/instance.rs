use apx_core::{
    http_url::HttpUrl,
    urls::normalize_origin,
};

pub fn parse_instance_url(url: &str) -> Result<HttpUrl, &'static str> {
    let origin = normalize_origin(url).map_err(|_| "invalid URL")?;
    let http_url = HttpUrl::parse(&origin)?;
    Ok(http_url)
}
