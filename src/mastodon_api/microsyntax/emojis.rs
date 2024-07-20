use std::collections::HashMap;

use regex::{Captures, Regex};

use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    emojis::queries::get_local_emojis_by_names,
    emojis::types::DbEmoji,
};

use super::links::is_inside_code_block;

// See also: EMOJI_NAME_RE in mitra_validators::emojis
const SHORTCODE_SEARCH_RE: &str = r"(?m):(?P<name>[a-zA-Z0-9._-]+):(?P<after>\s|$|\)|<)";

/// Finds emoji shortcodes in text
fn find_shortcodes(text: &str) -> Vec<String> {
    let shortcode_re = Regex::new(SHORTCODE_SEARCH_RE)
        .expect("regex should be valid");
    let mut emoji_names = vec![];
    for caps in shortcode_re.captures_iter(text) {
        let name_match = caps.name("name").expect("should have name group");
        if is_inside_code_block(&name_match, text) {
            // Ignore shortcodes inside code blocks
            continue;
        };
        let name = caps["name"].to_string();
        if !emoji_names.contains(&name) {
            emoji_names.push(name);
        };
    };
    emoji_names
}

pub async fn find_emojis(
    db_client: &impl DatabaseClient,
    text: &str,
) -> Result<HashMap<String, DbEmoji>, DatabaseError> {
    let emoji_names = find_shortcodes(text);
    // If shortcode doesn't exist in database, it is ignored
    let emojis = get_local_emojis_by_names(db_client, &emoji_names).await?;
    let mut emoji_map: HashMap<String, DbEmoji> = HashMap::new();
    for emoji in emojis {
        emoji_map.insert(emoji.emoji_name.clone(), emoji);
    };
    Ok(emoji_map)
}

pub fn replace_emojis(
    text: &str,
    custom_emoji_map: &HashMap<String, DbEmoji>,
) -> String {
    let shortcode_re = Regex::new(SHORTCODE_SEARCH_RE)
        .expect("regexp should be valid");
    let result = shortcode_re.replace_all(text, |caps: &Captures| {
        let name_match = caps.name("name").expect("should have name group");
        if is_inside_code_block(&name_match, text) {
            // Ignore shortcodes inside code blocks
            return caps[0].to_string();
        };
        let name = &caps["name"];
        if custom_emoji_map.contains_key(name) {
            // Don't replace custom emojis
            return caps[0].to_string();
        };
        if let Some(emoji) = emojis::get_by_shortcode(name) {
            // Replace
            return format!("{}{}", emoji, &caps["after"]);
        };
        // Leave unchanged if shortcode is not known
        caps[0].to_string()
    });
    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_WITH_EMOJIS: &str = concat!(
        "@user1@server1 text :emoji_name: :abc: ",
        "did:key:zXyvw (:key:) ",
        "<code>:abc:</code>",
    );

    #[test]
    fn test_find_shortcodes() {
        let emoji_names = find_shortcodes(TEXT_WITH_EMOJIS);

        assert_eq!(emoji_names, vec![
            "emoji_name",
            "abc",
            "key",
        ]);
    }

    #[test]
    fn test_replace_emojis() {
        let custom_emoji_map = HashMap::new();
        let result = replace_emojis(TEXT_WITH_EMOJIS, &custom_emoji_map);
        let expected_result = concat!(
            "@user1@server1 text :emoji_name: 🔤 ",
            "did:key:zXyvw (🔑) ",
            "<code>:abc:</code>",
        );
        assert_eq!(result, expected_result);
    }
}
