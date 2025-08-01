use regex::{Captures, Regex};

use mitra_activitypub::identifiers::local_tag_collection;

use super::parser::{is_inside_code_block, is_inside_link};

// See also: HASHTAG_NAME_RE in mitra_validators::tags
const HASHTAG_RE: &str = r"(?m)(?P<before>^|\s|>|[\(])#(?P<tag>[^\s<]+)";
const HASHTAG_SECONDARY_RE: &str = r"^(?P<tag>\p{Alphabetic}|[\p{Alphabetic}\d_]{2,})(?P<after>[\.,:;?!\)']*)$";

/// Finds anything that looks like a hashtag
pub fn find_hashtags(text: &str) -> Vec<String> {
    let hashtag_re = Regex::new(HASHTAG_RE)
        .expect("regexp should be valid");
    let hashtag_secondary_re = Regex::new(HASHTAG_SECONDARY_RE)
        .expect("regexp should be valid");
    let mut tags = vec![];
    for caps in hashtag_re.captures_iter(text) {
        let tag_match = caps.name("tag").expect("should have tag group");
        if is_inside_code_block(&tag_match, text) ||
            is_inside_link(&tag_match, text)
        {
            // Ignore hashtags inside code blocks and links
            continue;
        };
        if let Some(secondary_caps) = hashtag_secondary_re.captures(&caps["tag"]) {
            let tag_name = secondary_caps["tag"].to_string().to_lowercase();
            if !tags.contains(&tag_name) {
                tags.push(tag_name);
            };
        };
    };
    tags
}

/// Replaces hashtags with links
pub fn replace_hashtags(instance_url: &str, text: &str, tags: &[String]) -> String {
    let hashtag_re = Regex::new(HASHTAG_RE)
        .expect("regexp should be valid");
    let hashtag_secondary_re = Regex::new(HASHTAG_SECONDARY_RE)
        .expect("regexp should be valid");
    let result = hashtag_re.replace_all(text, |caps: &Captures| {
        let tag_match = caps.name("tag").expect("should have tag group");
        if is_inside_code_block(&tag_match, text) ||
            is_inside_link(&tag_match, text)
        {
            // Don't replace hashtags inside code blocks and links
            return caps[0].to_string();
        };
        if let Some(secondary_caps) = hashtag_secondary_re.captures(&caps["tag"]) {
            let before = caps["before"].to_string();
            let tag = secondary_caps["tag"].to_string();
            let tag_name = tag.to_lowercase();
            let after = secondary_caps["after"].to_string();
            if tags.contains(&tag_name) {
                let tag_url = local_tag_collection(instance_url, &tag_name);
                return format!(
                    r#"{}<a class="hashtag" href="{}" rel="tag">#{}</a>{}"#,
                    before,
                    tag_url,
                    tag,
                    after,
                );
            };
        };
        caps[0].to_string()
    });
    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";
    const TEXT_WITH_TAGS: &str = concat!(
        "@user1@server1 some text #TestTag.\n",
        "#TAG1 #tag1 #test_underscore #test*special ",
        "#test-tag # #123 ",
        "#aβcδ ",
        "more text (#tag2) text #tag3, #tag4:<br>",
        "end with #tag5",
    );

    #[test]
    fn test_find_hashtags() {
        let tags = find_hashtags(TEXT_WITH_TAGS);

        assert_eq!(tags, vec![
            "testtag",
            "tag1",
            "test_underscore",
            "123",
            "aβcδ",
            "tag2",
            "tag3",
            "tag4",
            "tag5",
        ]);
    }

    #[test]
    fn test_find_hashtags_single_letter() {
        let tags = find_hashtags("test #a");
        assert_eq!(tags, vec!["a"]);
    }

    #[test]
    fn test_find_hashtags_single_digit() {
        let tags = find_hashtags("test #1");
        assert_eq!(tags.is_empty(), true);
    }

    #[test]
    fn test_find_hashtags_single_underscore() {
        let tags = find_hashtags("test #_");
        assert_eq!(tags.is_empty(), true);
    }

    #[test]
    fn test_find_hashtags_multiple_characters_after() {
        let tags = find_hashtags("test (test #tag).");
        assert_eq!(tags, vec!["tag"]);
    }

    #[test]
    fn test_find_hashtags_inside_link() {
        let text = r#"test #111<a href="https://test">number #222</a>"#;
        let tags = find_hashtags(text);
        assert_eq!(tags, vec!["111"]);
    }

    #[test]
    fn test_replace_hashtags() {
        let tags = find_hashtags(TEXT_WITH_TAGS);
        let output = replace_hashtags(INSTANCE_URL, TEXT_WITH_TAGS, &tags);

        let expected_output = concat!(
            r#"@user1@server1 some text <a class="hashtag" href="https://example.com/collections/tags/testtag" rel="tag">#TestTag</a>."#, "\n",
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag1" rel="tag">#TAG1</a> "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag1" rel="tag">#tag1</a> "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/test_underscore" rel="tag">#test_underscore</a> #test*special "#,
            r#"#test-tag # <a class="hashtag" href="https://example.com/collections/tags/123" rel="tag">#123</a> "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/a%CE%B2c%CE%B4" rel="tag">#aβcδ</a> "#,
            r#"more text (<a class="hashtag" href="https://example.com/collections/tags/tag2" rel="tag">#tag2</a>) text "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag3" rel="tag">#tag3</a>, "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag4" rel="tag">#tag4</a>:<br>"#,
            r#"end with <a class="hashtag" href="https://example.com/collections/tags/tag5" rel="tag">#tag5</a>"#,
        );
        assert_eq!(output, expected_output);
    }
}
