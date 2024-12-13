pub fn remove_quotes(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|val| val.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_quotes() {
        assert_eq!(remove_quotes(r#""test""#), "test");
        assert_eq!(remove_quotes(r#"test"#), "test");
        assert_eq!(remove_quotes(r#""test"#), r#""test"#);
        assert_eq!(remove_quotes(r#"""test"""#), r#""test""#);
    }
}
