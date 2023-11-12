use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;

use ammonia::{
    rcdom::{Handle, NodeData, SerializableHandle},
    Builder,
    UrlRelative,
};
use html5ever::serialize::{serialize, SerializeOpts};

pub use ammonia::{clean_text as escape_html};

const EXTRA_URI_SCHEMES: [&str; 2] = [
    "gemini",
    "monero",
];

pub fn clean_html(
    unsafe_html: &str,
    allowed_classes: Vec<(&'static str, Vec<&'static str>)>,
) -> String {
    let mut builder = Builder::default();
    for (tag, classes) in allowed_classes.iter() {
        builder.add_allowed_classes(tag, classes);
    };
    let safe_html = builder
        // Remove external images to prevent tracking
        .rm_tags(&["img"])
        .add_url_schemes(&EXTRA_URI_SCHEMES)
        // Always add rel="noopener"
        .link_rel(Some("noopener"))
        .url_relative(UrlRelative::Deny)
        .clean(unsafe_html)
        .to_string();
    safe_html
}

fn node_to_string(node: Handle) -> String {
    // Taken from ammonia
    // https://github.com/rust-ammonia/ammonia/blob/98c2ebaca464ee9ee96acab43fc5a8ebb996106e/src/lib.rs#L2876
    let mut ret_val = Vec::new();
    let handle = SerializableHandle::from(node);
    serialize(&mut ret_val, &handle, SerializeOpts::default())
        .expect("writing to a string shouldn't fail");
    String::from_utf8(ret_val).expect("html5ever only supports UTF8")
}

fn insert_rel_noopener(node: &Handle) -> () {
    use html5ever::{local_name, ns, namespace_url, Attribute, QualName};
    let mut stack = vec![node.clone()];
    while let Some(node) = stack.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if &*name.local == "a" &&
                !attrs.borrow().iter().any(|attr| &*attr.name.local == "rel")
            {
                // Push rel=noopener if not already present
                attrs.borrow_mut().push(Attribute {
                    name: QualName::new(None, ns!(), local_name!("rel")),
                    value: "noopener".into(),
                });
            };
        };
        stack.extend(node.children.borrow_mut().clone().into_iter().rev());
    };
}

pub fn clean_html_strict(
    unsafe_html: &str,
    allowed_tags: &[&str],
    allowed_classes: Vec<(&'static str, Vec<&'static str>)>,
) -> String {
    let allowed_tags =
        HashSet::from_iter(allowed_tags.iter().copied());
    let mut allowed_classes_map = HashMap::new();
    for (tag, classes) in allowed_classes {
        allowed_classes_map.insert(
            tag,
            HashSet::from_iter(classes.into_iter()),
        );
    };
    let document = Builder::default()
        .tags(allowed_tags)
        .allowed_classes(allowed_classes_map)
        .add_url_schemes(&EXTRA_URI_SCHEMES)
        // Disable rel-insertion, allow rel attribute on <a>
        .link_rel(None)
        .url_relative(UrlRelative::Deny)
        .add_tag_attributes("a", &["rel"])
        .attribute_filter(|element, attribute, value| {
            match (element, attribute) {
                ("a", "rel") => {
                    // Remove everything except 'tag'
                    let mut rels: Vec<_> = value.split(' ')
                        .filter(|rel| *rel == "tag")
                        .collect();
                    // Always add rel="noopener"
                    rels.push("noopener");
                    Some(rels.join(" ").into())
                },
                _ => Some(value.into())
            }
        })
        .clean(unsafe_html);
    // .to_dom_node() is unstable (requires ammonia_unstable flag)
    // https://github.com/rust-ammonia/ammonia/issues/190
    let document_node = document.to_dom_node();
    insert_rel_noopener(&document_node);
    let safe_html = node_to_string(document_node);
    safe_html
}

pub fn clean_html_all(html: &str) -> String {
    let text = Builder::empty()
        .clean(html)
        .to_string();
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed_classes() -> Vec<(&'static str, Vec<&'static str>)> {
        vec![
            ("a", vec!["mention", "u-url"]),
            ("span", vec!["h-card"]),
        ]
    }

    #[test]
    fn test_clean_html() {
        let unsafe_html = concat!(
            r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="ugc">@<span>user</span></a></span> test</p>"#,
            r#"<p><img src="https://example.com/image.png" class="picture"></p>"#,
        );
        let expected_safe_html = concat!(
            r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="noopener">@<span>user</span></a></span> test</p>"#,
            r#"<p></p>"#,
        );
        let safe_html = clean_html(
            unsafe_html,
            allowed_classes(),
        );
        assert_eq!(safe_html, expected_safe_html);
    }

    #[test]
    fn test_clean_html_noopener() {
        let unsafe_html = r#"<a href="https://external.example">link</a>"#;
        let expected_safe_html = r#"<a href="https://external.example" rel="noopener">link</a>"#;
        let safe_html = clean_html(
            unsafe_html,
            allowed_classes(),
        );
        assert_eq!(safe_html, expected_safe_html);
    }

    #[test]
    fn test_clean_html_relative() {
        let unsafe_html = r#"<a href="/path">link</a>"#;
        let expected_safe_html = r#"<a rel="noopener">link</a>"#;
        let safe_html = clean_html(
            unsafe_html,
            allowed_classes(),
        );
        assert_eq!(safe_html, expected_safe_html);
    }

    #[test]
    fn test_clean_html_strict() {
        let unsafe_html = r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="ugc">@<span>user</span></a></span> test <b>bold</b><script>dangerous</script> with a <a href="https://server.example/tag" rel="tag">tag</a>, <a href="https://example.com" target="_blank" rel="noopener">link</a> and <code>code</code></p>"#;
        let safe_html = clean_html_strict(
            unsafe_html,
            &["a", "br", "code", "p", "span"],
            allowed_classes(),
        );
        assert_eq!(safe_html, r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="noopener">@<span>user</span></a></span> test bold with a <a href="https://server.example/tag" rel="tag noopener">tag</a>, <a href="https://example.com" rel="noopener">link</a> and <code>code</code></p>"#);
    }

    #[test]
    fn test_clean_html_strict_noopener() {
        // TODO: fix cleaner
        let unsafe_html = r#"<a href="https://external.example">link</a>"#;
        let expected_safe_html = r#"<a href="https://external.example" rel="noopener">link</a>"#;
        let safe_html = clean_html_strict(
            unsafe_html,
            &["a"],
            allowed_classes(),
        );
        assert_eq!(safe_html, expected_safe_html);
    }

    #[test]
    fn test_clean_html_all() {
        let html = r#"<p>test <b>bold</b><script>dangerous</script> with <a href="https://example.com">link</a> and <code>code</code></p>"#;
        let text = clean_html_all(html);
        assert_eq!(text, "test bold with link and code");
    }
}
