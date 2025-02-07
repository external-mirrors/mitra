use regex::Match;

pub fn is_inside_code_block(match_: &Match, text: &str) -> bool {
    // TODO: remove workaround.
    // Perform replacement only inside text nodes during markdown parsing
    let text_before = &text[0..match_.start()];
    let code_open = text_before.matches("<code>").count();
    let code_closed = text_before.matches("</code>").count();
    code_open > code_closed
}

#[cfg(test)]
mod tests {
    use regex::Regex;
    use super::*;

    #[test]
    fn test_is_inside_code_block() {
        let text = "abc<code>&&</code>xyz";
        let regexp = Regex::new("&&").unwrap();
        let mat = regexp.find(text).unwrap();
        assert_eq!(mat.start(), 9);
        let result = is_inside_code_block(&mat, text);
        assert_eq!(result, true);
    }
}
