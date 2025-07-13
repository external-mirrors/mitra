use unicode_segmentation::UnicodeSegmentation;

pub fn is_single_character(text: &str) -> bool {
    text.graphemes(true).count() == 1
}

pub fn trim_invisible(value: &str) -> &str {
    // Zero-width characters
    const CHARS: [char; 4] = ['\u{200b}', '\u{200c}', '\u{200d}', '\u{feff}'];
    value.trim().trim_matches(|chr| CHARS.contains(&chr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_single_character() {
        let text = "❤️";
        assert_eq!(is_single_character(text), true);
    }
}
