use unicode_segmentation::UnicodeSegmentation;

pub fn is_single_character(text: &str) -> bool {
    text.graphemes(true).count() == 1
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
