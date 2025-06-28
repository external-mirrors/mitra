use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;
use std::rc::Rc;

use ammonia::{
    rcdom::{Handle, NodeData, SerializableHandle},
    Builder,
    Document,
    UrlRelative,
};
use html2text::{
    from_read_with_decorator,
    render::text_renderer::TrivialDecorator,
};
use html5ever::serialize::{serialize, SerializeOpts};

pub use ammonia::{clean_text as escape_html};

pub(crate) const URI_SCHEMES: [&str; 11] = [
    // https://docs.rs/ammonia/3.3.0/ammonia/struct.Builder.html#method.url_schemes
    "bitcoin",
    "http",
    "https",
    "irc",
    "magnet",
    "mailto",
    "xmpp",
    // Extra
    "gemini",
    "monero",
    "mumble",
    // Experimental (could be removed in the future)
    "nex", // https://nex.nightfall.city/nex/info/
];

fn document_to_node(document: &Document) -> Handle {
    // .to_dom_node() is unstable (requires ammonia_unstable flag)
    // https://github.com/rust-ammonia/ammonia/issues/190
    document.to_dom_node()
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

fn iter_nodes<F>(root: &Handle, func: F) -> ()
    where F: Fn(&Handle) -> ()
{
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        func(&node);
        stack.extend(node.children.borrow_mut().clone().into_iter().rev());
    };
}

mod dom {
    use ammonia::rcdom::{Handle, Node, NodeData};
    use html5ever::{
        namespace_url,
        ns,
        tendril::format_tendril,
        Attribute,
        QualName,
    };

    pub fn create_attribute(name: &str, value: &str) -> Attribute {
        Attribute {
            name: QualName::new(None, ns!(), name.into()),
            value: value.into(),
        }
    }

    pub fn create_text(text: &str) -> Handle {
        let contents = format_tendril!("{text}").into();
        let node_data = NodeData::Text { contents };
        Node::new(node_data)
    }
}

/// Replaces all <img> tags with links
fn replace_images(root: &Handle) -> () {
    iter_nodes(root, |node| {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if &*name.local == "img" {
                let maybe_parent = node.parent
                    .take()
                    .and_then(|weak| weak.upgrade());
                if let Some(parent) = maybe_parent {
                    // Get index of current element
                    let maybe_index = parent
                        .children
                        .borrow()
                        .iter()
                        .enumerate()
                        .find(|&(_, child)| Rc::ptr_eq(child, node))
                        .map(|(index, _)| index);
                    if let Some(index) = maybe_index {
                        // Remove <img> element
                        parent.children.borrow_mut().remove(index);
                        node.parent.set(None);
                        let image_src = attrs.borrow().iter()
                            .find(|attr| &*attr.name.local == "src")
                            .map(|attr| attr.value.to_string())
                            .unwrap_or("image".to_string());
                        // Adding only a text node because with <a>
                        // wrapper it gets lost and a warning is logged:
                        // `node with weird namespace Atom('' type=static)`
                        let text_node = dom::create_text(&image_src);
                        text_node.parent.set(Some(Rc::downgrade(&parent)));
                        parent.children.borrow_mut().insert(index, text_node);
                    };
                };
            };
        };
    });
}

fn rel_attribute_filter<'u>(
    element: &str,
    attribute: &str,
    value: &'u str,
) -> Option<Cow<'u, str>> {
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
}

pub fn clean_html(
    unsafe_html: &str,
    allowed_classes: Vec<(&'static str, Vec<&'static str>)>,
) -> String {
    let mut builder = Builder::default();
    for (tag, classes) in allowed_classes.iter() {
        builder.add_allowed_classes(tag, classes);
    };
    let document = builder
        .url_schemes(URI_SCHEMES.into())
        // Disable rel-insertion, allow rel attribute on <a>
        .link_rel(None)
        .add_tag_attributes("a", &["rel"])
        .attribute_filter(rel_attribute_filter)
        .url_relative(UrlRelative::Deny)
        .clean(unsafe_html);
    let document_node = document_to_node(&document);
    // Insert rel=noopener if not present
    // attribute_filter can only modify attribute value
    insert_rel_noopener(&document_node);
    // Replace external images to prevent tracking
    replace_images(&document_node);
    let safe_html = node_to_string(document_node);
    safe_html
}

fn insert_rel_noopener(root: &Handle) -> () {
    iter_nodes(root, |node| {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if &*name.local == "a" &&
                !attrs.borrow().iter().any(|attr| &*attr.name.local == "rel")
            {
                // Push rel=noopener if not already present
                let rel = dom::create_attribute("rel", "noopener");
                attrs.borrow_mut().push(rel);
            };
        };
    });
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
        .url_schemes(URI_SCHEMES.into())
        // Disable rel-insertion, allow rel attribute on <a>
        .link_rel(None)
        .add_tag_attributes("a", &["rel"])
        .attribute_filter(rel_attribute_filter)
        .url_relative(UrlRelative::Deny)
        .clean(unsafe_html);
    let document_node = document_to_node(&document);
    // Insert rel=noopener if not present
    // attribute_filter can only modify attribute value
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

fn html_to_text(html: &str) -> String {
    let decorator = TrivialDecorator::new();
    from_read_with_decorator(html.as_bytes(), usize::MAX, decorator)
}

pub fn extract_title(html: &str, length: usize) -> String {
    let first_line = html_to_text(html)
        .lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
        .unwrap_or("-".to_string());
    let mut title: String = first_line
        .chars()
        .take(length)
        .collect();
    if title.len() == length && first_line.len() != length {
        title += "...";
    };
    title
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
            r#"<p>https://example.com/image.png</p>"#,
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
    fn test_clean_html_with_image_with_link() {
        let unsafe_html = r#"<p><a href="https://external.example/page"><img src="https://external.example/image.png"></a></p>"#;
        let expected_safe_html = r#"<p><a href="https://external.example/page" rel="noopener">https://external.example/image.png</a></p>"#;
        let safe_html = clean_html(
            unsafe_html,
            allowed_classes(),
        );
        assert_eq!(safe_html, expected_safe_html);
    }

    #[test]
    fn test_clean_html_with_hashtag() {
        let unsafe_html = r#"<p>test <a href="https://server.example/tag" rel="tag">#tag</a></p>"#;
        let expected_safe_html = r#"<p>test <a href="https://server.example/tag" rel="tag noopener">#tag</a></p>"#;
        let safe_html = clean_html(
            unsafe_html,
            allowed_classes(),
        );
        assert_eq!(safe_html, expected_safe_html);
    }

    #[test]
    fn test_clean_html_gemini_link() {
        let unsafe_html = r#"<p>test <a href="gemini://geminispace.info">gemini://geminispace.info</a>.</p>"#;
        let expected_safe_html = r#"<p>test <a href="gemini://geminispace.info" rel="noopener">gemini://geminispace.info</a>.</p>"#;
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

    #[test]
    fn test_html_to_text() {
        let html = r#"<h1>heading</h1><p>next line</p>"#;
        let text = html_to_text(html);
        assert_eq!(text, "heading\n\nnext line\n");
    }
}
