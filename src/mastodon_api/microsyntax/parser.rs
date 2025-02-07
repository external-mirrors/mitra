use regex::Match;

fn is_inside_tag(match_: &Match, tag: &str, text: &str) -> bool {
    // TODO: remove workaround.
    // Perform replacement only inside text nodes during markdown parsing
    let text_before = &text[0..match_.start()];
    let tag_open = format!("<{tag}");
    let tag_close = format!("</{tag}>");
    let tag_open_count = text_before.matches(&tag_open).count();
    let tag_close_count = text_before.matches(&tag_close).count();
    tag_open_count > tag_close_count
}

pub fn is_inside_code_block(match_: &Match, text: &str) -> bool {
    is_inside_tag(match_, "code", text)
}

pub fn is_inside_link(match_: &Match, text: &str) -> bool {
    is_inside_tag(match_, "a", text)
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

    #[test]
    fn test_is_inside_link() {
        let text = r#"abc<a href="https://test">#tag</a>xyz<a>link</a>"#;
        let regexp = Regex::new("#tag").unwrap();
        let match_ = regexp.find(text).unwrap();
        assert_eq!(match_.start(), 26);
        let result = is_inside_link(&match_, text);
        assert_eq!(result, true);
    }
}
