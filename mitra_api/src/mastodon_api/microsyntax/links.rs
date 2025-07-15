use indexmap::IndexMap;
use regex::{Captures, Regex};

use mitra_activitypub::{
    identifiers::{canonicalize_id, compatible_post_object_id},
    importers::get_post_by_object_id,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::{
        helpers::can_link_post,
        types::Post,
    },
};
use mitra_validators::posts::LINK_LIMIT;

use super::parser::is_inside_code_block;

// MediaWiki-like syntax: [[url|text]]
const OBJECT_LINK_SEARCH_RE: &str = r"(?m)\[\[(?P<url>[^\s\|]+?)(\|(?P<text>.+?))?\]\]";

/// Finds everything that looks like an object link
fn find_object_links(text: &str) -> Vec<String> {
    let link_re = Regex::new(OBJECT_LINK_SEARCH_RE)
        .expect("regexp should be valid");
    let mut links = vec![];
    for caps in link_re.captures_iter(text) {
        let url_match = caps.name("url").expect("should have url group");
        if is_inside_code_block(&url_match, text) {
            // Ignore links inside code blocks
            continue;
        };
        let url = caps["url"].to_string();
        if !links.contains(&url) {
            links.push(url);
        };
    };
    links
}

pub async fn find_linked_posts(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    text: &str,
) -> Result<IndexMap<String, Post>, DatabaseError> {
    let links = find_object_links(text);
    let mut link_map: IndexMap<String, Post> = IndexMap::new();
    let mut counter = 0;
    for url in links {
        if counter > LINK_LIMIT {
            // Limit the number of queries
            break;
            // TODO: single database query
        };
        let Ok(canonical_id) = canonicalize_id(&url) else {
            // Skip invalid IDs
            continue;
        };
        match get_post_by_object_id(
            db_client,
            instance_url,
            &canonical_id,
        ).await {
            Ok(post) => {
                if !can_link_post(&post) {
                    continue;
                };
                link_map.insert(url, post);
            },
            // If post doesn't exist in database, link is ignored
            Err(DatabaseError::NotFound(_)) => continue,
            Err(other_error) => return Err(other_error),
        };
        counter += 1;
    };
    Ok(link_map)
}

pub fn replace_object_links(
    link_map: &IndexMap<String, Post>,
    text: &str,
) -> String {
    let mention_re = Regex::new(OBJECT_LINK_SEARCH_RE)
        .expect("regexp should be valid");
    let result = mention_re.replace_all(text, |caps: &Captures| {
        let url_match = caps.name("url").expect("should have url group");
        if is_inside_code_block(&url_match, text) {
            // Don't replace inside code blocks
            return caps[0].to_string();
        };
        let url = caps["url"].to_string();
        let link_text = caps.name("text")
            .map(|match_| match_.as_str())
            .unwrap_or(&url)
            .to_string();
        if link_map.contains_key(&url) {
            return format!(r#"<a href="{0}">{1}</a>"#, url, link_text);
        };
        // Leave unchanged if post does not exist
        caps[0].to_string()
    });
    result.to_string()
}

pub fn insert_quote(
    instance_url: &str,
    content: &str,
    quote_of: &Post,
) -> String {
    format!(
        r#"{content}<p>RE: <a href="{0}">{0}</a></p>"#,
        compatible_post_object_id(instance_url, quote_of),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_WITH_OBJECT_LINKS: &str = concat!(
        "test [[https://example.org/1]] link ",
        "test link with [[https://example.org/1|text]] ",
        "test ([[https://example.org/2]])",
    );

    #[test]
    fn test_find_object_links() {
        let results = find_object_links(TEXT_WITH_OBJECT_LINKS);
        assert_eq!(results, vec![
            "https://example.org/1",
            "https://example.org/2",
        ]);
    }

    #[test]
    fn test_replace_object_links() {
        let mut link_map = IndexMap::new();
        link_map.insert("https://example.org/1".to_string(), Post::default());
        link_map.insert("https://example.org/2".to_string(), Post::default());
        let result = replace_object_links(&link_map, TEXT_WITH_OBJECT_LINKS);
        let expected_result = concat!(
            r#"test <a href="https://example.org/1">https://example.org/1</a> link "#,
            r#"test link with <a href="https://example.org/1">text</a> "#,
            r#"test (<a href="https://example.org/2">https://example.org/2</a>)"#,
        );
        assert_eq!(result, expected_result);
    }
}
