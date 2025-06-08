//! Parsing object identifiers.

use std::str::FromStr;

use regex::{Captures, Regex};
use thiserror::Error;

use apx_core::http_url::HttpUrl;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct PathError(&'static str);

pub trait FromCaptures {
    fn from_captures(caps: Captures) -> Result<Self, PathError>
        where Self: Sized;
}

fn extract(caps: Captures) -> Result<Vec<&str>, PathError> {
    let substrings = caps.iter().skip(1)
        .map(|maybe_match| {
            maybe_match
                .map(|match_| match_.as_str())
                .ok_or(PathError("invalid path"))
        })
        .collect::<Result<_, _>>()?;
    Ok(substrings)
}

impl<A: FromStr> FromCaptures for (A,) {
    fn from_captures(caps: Captures) -> Result<Self, PathError> {
        if let [a] = extract(caps)?[..] {
            let a = a.parse()
                .map_err(|_| PathError("invalid path segment"))?;
            Ok((a,))
        } else {
            Err(PathError("unexpected number of groups"))
        }
    }
}

impl<A: FromStr, B: FromStr> FromCaptures for (A, B) {
    fn from_captures(caps: Captures) -> Result<Self, PathError> {
        if let [a, b] = extract(caps)?[..] {
            let a = a.parse()
                .map_err(|_| PathError("invalid path segment"))?;
            let b = b.parse()
                .map_err(|_| PathError("invalid path segment"))?;
            Ok((a, b))
        } else {
            Err(PathError("unexpected number of groups"))
        }
    }
}

pub fn parse_object_id<T: FromCaptures>(
    object_id: &str,
    path_re: Regex,
) -> Result<(String, T), PathError> {
    let url = HttpUrl::parse(object_id)
        .map_err(|_| PathError("invalid URL"))?;
    let base_url = url.base();
    let path = url.to_relative();
    let path_caps = path_re.captures(&path)
        .ok_or(PathError("invalid path"))?;
    let value = T::from_captures(path_caps)?;
    Ok((base_url, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_object_id() {
        let actor_id = "https://social.example/users/test";
        let path_re = Regex::new("^/users/(?P<username>[0-9a-z_]+)$").unwrap();
        let (base_url, (username,)) = parse_object_id::<(String,)>(
            actor_id,
            path_re,
        ).unwrap();
        assert_eq!(base_url, "https://social.example");
        assert_eq!(username, "test");
    }
}
