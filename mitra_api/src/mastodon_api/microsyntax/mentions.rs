use apx_core::url::hostname::encode_hostname;
use apx_sdk::addresses::WebfingerAddress;
use indexmap::IndexMap;
use regex::{Captures, Regex};

use mitra_activitypub::{
    authority::Authority,
    identifiers::profile_actor_url,
};
use mitra_adapters::profiles::profile_address;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::get_profiles_by_accts,
    profiles::types::DbActorProfile,
};

use super::parser::is_inside_code_block;

// IDNs are allowed, but encoded during parsing.
// See also: USERNAME_RE in mitra_validators::profiles
const MENTION_SEARCH_RE: &str = r"(?m)(?P<before>^|\s|>|[\(])@(?P<mention>[^\s<]+)";
// username must not end with "."
const MENTION_SEARCH_SECONDARY_RE: &str = r"(?x)
    ^(?P<username>[A-Za-z0-9\-\._]*[A-Za-z0-9_])
    (@(?P<hostname>[\w\.-]+\w|[0-9\.]+|\[[0-9a-f:]+\]))?
    (?P<after>[\.,:;?!\)']*)$
    ";

fn caps_to_address(
    instance_hostname: &str,
    caps: &Captures,
) -> Option<WebfingerAddress> {
    let username = &caps["username"];
    let hostname = if let Ok(maybe_hostname) = caps.name("hostname")
        .map(|match_| encode_hostname(match_.as_str()))
        .transpose()
    {
        maybe_hostname.unwrap_or(instance_hostname.to_string())
    } else {
        // Invalid hostname
        return None;
    };
    let webfinger_address = WebfingerAddress::new_unchecked(
        username,
        &hostname,
    );
    Some(webfinger_address)
}

/// Finds everything that looks like a mention
fn find_mentions(
    instance_hostname: &str,
    text: &str,
) -> Vec<WebfingerAddress> {
    let mention_re = Regex::new(MENTION_SEARCH_RE)
        .expect("regexp should be valid");
    let mention_secondary_re = Regex::new(MENTION_SEARCH_SECONDARY_RE)
        .expect("regexp should be valid");
    let mut mentions = vec![];
    for caps in mention_re.captures_iter(text) {
        let mention_match = caps.name("mention").expect("should have mention group");
        if is_inside_code_block(&mention_match, text) {
            // No mentions inside code blocks
            continue;
        };
        if let Some(secondary_caps) = mention_secondary_re.captures(&caps["mention"]) {
            let Some(address) = caps_to_address(instance_hostname, &secondary_caps) else {
                // Invalid mention
                continue;
            };
            if !mentions.contains(&address) {
                mentions.push(address);
            };
        };
    };
    mentions
}

pub async fn find_mentioned_profiles(
    db_client: &impl DatabaseClient,
    instance_hostname: &str,
    text: &str,
) -> Result<IndexMap<WebfingerAddress, DbActorProfile>, DatabaseError> {
    let mentions = find_mentions(instance_hostname, text);
    let mut accts = vec![];
    for address in mentions {
        accts.push(address.acct(instance_hostname));
    };
    // If acct doesn't exist in database, mention is ignored
    let profiles = get_profiles_by_accts(db_client, accts).await?;
    let mut mention_map = IndexMap::new();
    for profile in profiles {
        let Some(address) = profile_address(instance_hostname, &profile) else {
            // get_profiles_by_accts should not return profiles without address
            return Err(DatabaseError::type_error());
        };
        mention_map.insert(address, profile);
    };
    Ok(mention_map)
}

pub fn replace_mentions(
    mention_map: &IndexMap<WebfingerAddress, DbActorProfile>,
    instance_hostname: &str,
    authority: &Authority,
    text: &str,
) -> String {
    let mention_re = Regex::new(MENTION_SEARCH_RE)
        .expect("regexp should be valid");
    let mention_secondary_re = Regex::new(MENTION_SEARCH_SECONDARY_RE)
        .expect("regexp should be valid");
    let result = mention_re.replace_all(text, |caps: &Captures| {
        let mention_match = caps.name("mention").expect("should have mention group");
        if is_inside_code_block(&mention_match, text) {
            // Don't replace mentions inside code blocks
            return caps[0].to_string();
        };
        if let Some(secondary_caps) = mention_secondary_re.captures(&caps["mention"]) {
            let address = if let Some(address) = caps_to_address(instance_hostname, &secondary_caps) {
                address
            } else {
                // Invalid mention
                return caps[0].to_string();
            };
            if let Some(profile) = mention_map.get(&address) {
                // Replace with a link to profile.
                // Actor URL may differ from actor ID.
                let url = profile_actor_url(authority, profile);
                #[allow(clippy::to_string_in_format_args)]
                return format!(
                    // https://microformats.org/wiki/h-card
                    r#"{}<span class="h-card"><a class="u-url mention" href="{}">@{}</a></span>{}"#,
                    caps["before"].to_string(),
                    url,
                    profile.username,
                    secondary_caps["after"].to_string(),
                );
            };
        };
        // Leave unchanged if actor is not known
        caps[0].to_string()
    });
    result.to_string()
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActor;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "server1.com";
    const INSTANCE_URI: &str = "https://server1.com";
    const TEXT_WITH_MENTIONS: &str = concat!(
        "@user1 ",
        "@user_x@server1.com,<br>",
        "(@user2@server2.com boosted) ",
        "@user3@δοκιμή.example.\n",
        "@@invalid@server2.com ",
        "@test@server3.com@nospace@server4.com ",
        "@ email@unknown.org ",
        "@user2@server2.com copy ",
        "some text",
    );

    fn find_mentions(instance_hostname: &str, text: &str) -> Vec<String> {
        super::find_mentions(instance_hostname, text)
            .into_iter()
            .map(|address| address.acct(instance_hostname))
            .collect()
    }

    #[test]
    fn test_find_mentions() {
        let mentions = find_mentions(INSTANCE_HOSTNAME, TEXT_WITH_MENTIONS);
        assert_eq!(mentions, vec![
            "user1",
            "user_x",
            "user2@server2.com",
            "user3@xn--jxalpdlp.example",
        ]);
    }

    #[test]
    fn test_find_mentions_single_letter_mention() {
        let text = "Hey @p";
        let mentions = find_mentions(INSTANCE_HOSTNAME, text);
        assert_eq!(mentions, vec!["p"]);
    }

    #[test]
    fn test_find_mentions_short_mention_followed_by_dot() {
        let text = "Hey @user.";
        let mentions = find_mentions(INSTANCE_HOSTNAME, text);
        assert_eq!(mentions, vec!["user"]);
    }

    #[test]
    fn test_find_mentions_multiple_characters_after() {
        let text = "test (test @user@server.example).";
        let mentions = find_mentions(INSTANCE_HOSTNAME, text);
        assert_eq!(mentions, vec!["user@server.example"]);
    }

    #[test]
    fn test_find_mentions_ipv6_hostname() {
        let text = "Hey @user@[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]!";
        let mentions = find_mentions(INSTANCE_HOSTNAME, text);
        assert_eq!(
            mentions,
            vec!["user@[319:3cf0:dd1d:47b9:20c:29ff:fe2c:39be]"],
        );
    }

    #[test]
    fn test_replace_mentions() {
        // Local actors
        let profile_1 = DbActorProfile::local_for_test("user1");
        let profile_2 = DbActorProfile::local_for_test("user_x");
        // Remote actors
        let profile_3 = DbActorProfile::remote_for_test_with_data(
            "user2",
            DbActor {
                id: "https://server2.com/actors/user2".to_string(),
                url: Some("https://server2.com/@user2".to_string()),
                ..Default::default()
            },
        );
        let profile_4 = DbActorProfile::remote_for_test_with_data(
            "user3",
            DbActor {
                id: "https://xn--jxalpdlp.example/actors/user3".to_string(),
                url: Some("https://xn--jxalpdlp.example/@user3".to_string()),
                ..Default::default()
            },
        );
        let mention_map = IndexMap::from([
            (profile_address(INSTANCE_HOSTNAME, &profile_1).unwrap(), profile_1),
            (profile_address(INSTANCE_HOSTNAME, &profile_2).unwrap(), profile_2),
            (profile_address(INSTANCE_HOSTNAME, &profile_3).unwrap(), profile_3),
            (profile_address(INSTANCE_HOSTNAME, &profile_4).unwrap(), profile_4),
        ]);
        let authority = Authority::server_unchecked(INSTANCE_URI);
        let result = replace_mentions(
            &mention_map,
            INSTANCE_HOSTNAME,
            &authority,
            TEXT_WITH_MENTIONS,
        );

        let expected_result = concat!(
            r#"<span class="h-card"><a class="u-url mention" href="https://server1.com/users/user1">@user1</a></span> "#,
            r#"<span class="h-card"><a class="u-url mention" href="https://server1.com/users/user_x">@user_x</a></span>,<br>"#,
            r#"(<span class="h-card"><a class="u-url mention" href="https://server2.com/@user2">@user2</a></span> boosted) "#,
            r#"<span class="h-card"><a class="u-url mention" href="https://xn--jxalpdlp.example/@user3">@user3</a></span>."#, "\n",
            r#"@@invalid@server2.com @test@server3.com@nospace@server4.com "#,
            r#"@ email@unknown.org <span class="h-card"><a class="u-url mention" href="https://server2.com/@user2">@user2</a></span> copy some text"#,
        );
        assert_eq!(result, expected_result);
    }
}
