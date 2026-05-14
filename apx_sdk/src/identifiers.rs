//! Parsing object identifiers.

use std::str::FromStr;

use regex::{Captures, Regex};
use thiserror::Error;

use apx_core::url::{
    canonical::CanonicalUri,
};

#[derive(Debug, Error)]
#[error("{0}")]
pub struct PathError(&'static str);

pub trait FromCaptures {
    fn from_captures(caps: Captures) -> Result<Self, PathError>
        where Self: Sized;
}

fn extract(caps: Captures<'_>) -> Result<Vec<&str>, PathError> {
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

/// Parses local object ID and extracts values from its path component
pub fn parse_object_id<T: FromCaptures>(
    object_id: &str,
    path_re: Regex,
) -> Result<(String, T), PathError> {
    let uri = CanonicalUri::parse_canonical(object_id)
        .map_err(|_| PathError("invalid URI"))?;
    let (base_uri, path) = match uri {
        CanonicalUri::Ap(ap_uri) => (ap_uri.base(), ap_uri.relative_uri()),
        CanonicalUri::Http(http_uri) => (http_uri.base(), http_uri.to_relative()),
    };
    let path_caps = path_re.captures(&path)
        .ok_or(PathError("invalid path"))?;
    let value = T::from_captures(path_caps)?;
    Ok((base_uri, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_object_id_http() {
        let actor_id = "https://social.example/users/test";
        let path_re = Regex::new(r"^/users/(?P<username>[0-9A-Za-z_\-]+)$").unwrap();
        let (base_uri, (username,)) = parse_object_id::<(String,)>(
            actor_id,
            path_re,
        ).unwrap();
        assert_eq!(base_uri, "https://social.example");
        assert_eq!(username, "test");
    }

    #[test]
    fn test_parse_object_id_ap() {
        let actor_id = "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actors/123";
        let path_re = Regex::new(r"^/actors/(?P<id>[0-9]+)$").unwrap();
        let (base_uri, (local_id,)) = parse_object_id::<(u64,)>(
            actor_id,
            path_re,
        ).unwrap();
        assert_eq!(base_uri, "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6");
        assert_eq!(local_id, 123);
    }
}
