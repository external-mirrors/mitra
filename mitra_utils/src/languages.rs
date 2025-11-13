pub use isolang::Language;

// https://en.wikipedia.org/wiki/IETF_language_tag
pub fn parse_language_tag(value: &str) -> Option<Language> {
    let code = value.split_once('-')
        .map(|(code, _)| code)
        .unwrap_or(value);
    Language::from_639_1(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_language_tag() {
        // https://www.w3.org/TR/activitystreams-vocabulary/#dfn-content
        let value = "zh-Hans";
        let result = parse_language_tag(value);
        assert_eq!(result, Some(Language::Zho));
    }
}
