use std::str::FromStr;

use regex::Regex;

use apx_core::{
    ap_url::is_ap_url,
    did::Did,
    http_url::normalize_http_url,
    urls::encode_hostname,
};
use apx_sdk::addresses::WebfingerAddress;
use mitra_activitypub::{
    errors::HandlerError,
    identifiers::parse_local_object_id,
    importers::{
        import_post,
        import_profile_by_webfinger_address,
        ActorIdResolver,
        ApClient,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::{
        queries::search_posts,
        helpers::{can_view_post, get_local_post_by_id},
        types::Post,
    },
    profiles::queries::{
        search_profiles,
        search_profiles_by_did_only,
        search_profiles_by_ethereum_address,
    },
    profiles::types::DbActorProfile,
    tags::queries::search_tags,
    users::types::User,
};
use mitra_services::{
    ethereum::utils::validate_ethereum_address,
};
use mitra_validators::{
    errors::ValidationError,
    profiles::validate_hostname,
};

const SEARCH_FETCHER_TIMEOUT: u64 = 15;

enum SearchQuery {
    Text(String),
    ProfileQuery(String, Option<String>),
    TagQuery(String),
    Url(String),
    WalletAddress(String),
    Did(Did),
    Unknown,
}

fn parse_url_query(query: &str) -> Result<String, ValidationError> {
    let url = if is_ap_url(query) {
        query.to_string()
    } else {
        normalize_http_url(query)
            .map_err(ValidationError)?
    };
    Ok(url)
}

fn parse_profile_query(query: &str) ->
    Result<(String, Option<String>), ValidationError>
{
    // Only valid usernames are recognized
    // See also: USERNAME_RE in mitra_validators::profiles
    let acct_query_re =
        Regex::new(r"^(@|!|acct:)?(?P<username>[A-Za-z0-9\-\._]+)(@(?P<hostname>[^@\s]*))?$")
            .expect("regexp should be valid");
    let acct_query_caps = acct_query_re.captures(query)
        .ok_or(ValidationError("invalid profile query"))?;
    let username = acct_query_caps.name("username")
        .ok_or(ValidationError("invalid profile query"))?
        .as_str().to_string();
    let maybe_hostname = acct_query_caps.name("hostname")
        .map(|val| val.as_str())
        .filter(|val| !val.is_empty())
        // Normalize domain name
        .map(encode_hostname)
        .transpose()
        .map_err(|_| ValidationError("invalid domain name"))?;
    if let Some(ref hostname) = maybe_hostname {
        validate_hostname(hostname)?;
    };
    Ok((username, maybe_hostname))
}

fn parse_tag_query(query: &str) -> Result<String, ValidationError> {
    let tag_query_re = Regex::new(r"^#(?P<tag>\w+)$")
        .expect("regexp should be valid");
    let tag_query_caps = tag_query_re.captures(query)
        .ok_or(ValidationError("invalid tag query"))?;
    let tag = tag_query_caps.name("tag")
        .ok_or(ValidationError("invalid tag query"))?
        .as_str().to_string();
    Ok(tag)
}

fn parse_text_query(query: &str) -> Result<String, ValidationError> {
    let text_query_re = Regex::new(r"^>(?P<text>.+)$")
        .expect("regexp should be valid");
    let captures = text_query_re.captures(query)
        .ok_or(ValidationError("invalid text query"))?;
    let text = captures["text"].to_string();
    Ok(text)
}

fn parse_search_query(search_query: &str) -> SearchQuery {
    let search_query = search_query.trim();
    if let Ok(did) = Did::from_str(search_query) {
        return SearchQuery::Did(did);
    };
    if let Ok(url) = parse_url_query(search_query) {
        return SearchQuery::Url(url);
    };
    // TODO: support other currencies
    if validate_ethereum_address(&search_query.to_lowercase()).is_ok() {
        return SearchQuery::WalletAddress(search_query.to_string());
    };
    if let Ok(tag) = parse_tag_query(search_query) {
        return SearchQuery::TagQuery(tag);
    };
    if let Ok(text) = parse_text_query(search_query) {
        return SearchQuery::Text(text);
    };
    // Profile query may not start with @,
    // and should be tried after all others
    if let Ok((username, maybe_hostname)) = parse_profile_query(search_query) {
        return SearchQuery::ProfileQuery(username, maybe_hostname);
    };
    SearchQuery::Unknown
}

async fn search_profiles_or_import(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    username: String,
    mut maybe_hostname: Option<String>,
    resolve: bool,
    limit: u16,
    offset: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    if let Some(ref hostname) = maybe_hostname {
        if hostname == &ap_client.instance.hostname() {
            // This is a local profile
            maybe_hostname = None;
        };
    };
    let mut profiles = search_profiles(
        db_client,
        &username,
        maybe_hostname.as_ref(),
        limit,
        offset,
    ).await?;
    if profiles.is_empty() && resolve {
        if let Some(hostname) = maybe_hostname {
            let webfinger_address =
                WebfingerAddress::new_unchecked(&username, &hostname);
            match import_profile_by_webfinger_address(
                ap_client,
                db_client,
                &webfinger_address,
            ).await {
                Ok(profile) => {
                    profiles.push(profile);
                },
                Err(HandlerError::DatabaseError(db_error)) => {
                    // Propagate database errors
                    return Err(db_error);
                },
                Err(other_error) => {
                    log::warn!(
                        "failed to import profile {}: {}",
                        webfinger_address,
                        other_error,
                    );
                },
            };
        };
    };
    Ok(profiles)
}

/// Finds post by its object ID
async fn find_post_by_url(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    url: &str,
) -> Result<Option<Post>, DatabaseError> {
    let maybe_post = match parse_local_object_id(&ap_client.instance.url(), url) {
        Ok(post_id) => {
            // Local URL
            match get_local_post_by_id(db_client, post_id).await {
                Ok(post) => Some(post),
                Err(DatabaseError::NotFound(_)) => None,
                Err(other_error) => return Err(other_error),
            }
        },
        Err(_) => {
            match import_post(
                ap_client,
                db_client,
                url.to_string(),
                None,
            ).await {
                Ok(post) => Some(post),
                Err(err) => {
                    log::warn!("{}", err);
                    None
                },
            }
        },
    };
    Ok(maybe_post)
}

async fn find_profile_by_url(
    ap_client: &ApClient,
    db_client: &mut impl DatabaseClient,
    url: &str,
) -> Result<Option<DbActorProfile>, DatabaseError> {
    let maybe_profile = match ActorIdResolver::default().resolve(
        ap_client,
        db_client,
        url,
    ).await {
        Ok(profile) => Some(profile),
        Err(HandlerError::DatabaseError(DatabaseError::NotFound(_))) => {
            // Local profile not found
            None
        },
        Err(HandlerError::DatabaseError(db_error)) => return Err(db_error),
        Err(other_error) => {
            // LocalObject, FetchError, ValidationError, StorageError
            log::warn!("{}", other_error);
            None
        },
    };
    Ok(maybe_profile)
}

type SearchResults = (Vec<DbActorProfile>, Vec<Post>, Vec<String>);

pub async fn search(
    config: &Config,
    current_user: &User,
    db_client: &mut impl DatabaseClient,
    search_query: &str,
    limit: u16,
    offset: u16,
) -> Result<SearchResults, DatabaseError> {
    let mut profiles = vec![];
    let mut posts = vec![];
    let mut tags = vec![];
    let mut ap_client = ApClient::new(config, db_client).await?;
    ap_client.instance.fetcher_timeout = SEARCH_FETCHER_TIMEOUT;
    match parse_search_query(search_query) {
        SearchQuery::Text(text) => {
            posts = search_posts(
                db_client,
                &text,
                current_user.id,
                limit,
                offset,
            ).await?;
        },
        SearchQuery::ProfileQuery(username, maybe_hostname) => {
            profiles = search_profiles_or_import(
                &ap_client,
                db_client,
                username,
                maybe_hostname,
                true,
                limit,
                offset,
            ).await?;
        },
        SearchQuery::TagQuery(tag) => {
            tags = search_tags(
                db_client,
                &tag,
                limit,
                offset,
            ).await?;
        },
        SearchQuery::Url(url) => {
            let maybe_post = find_post_by_url(
                &ap_client,
                db_client,
                &url,
            ).await?;
            if let Some(post) = maybe_post {
                if can_view_post(
                    db_client,
                    Some(&current_user.profile),
                    &post,
                ).await? {
                    posts = vec![post];
                };
            } else {
                let maybe_profile = find_profile_by_url(
                    &ap_client,
                    db_client,
                    &url,
                ).await?;
                if let Some(profile) = maybe_profile {
                    profiles = vec![profile];
                };
            };
        },
        SearchQuery::WalletAddress(address) => {
            // Search by wallet address, assuming it's ethereum address
            // TODO: support other currencies
            profiles = search_profiles_by_ethereum_address(
                db_client,
                &address,
                false,
            ).await?;
        },
        SearchQuery::Did(did) => {
            profiles = search_profiles_by_did_only(
                db_client,
                &did,
            ).await?;
        },
        SearchQuery::Unknown => (), // ignore
    };
    Ok((profiles, posts, tags))
}

pub async fn search_profiles_only(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    search_query: &str,
    resolve: bool,
    limit: u16,
    offset: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let (username, maybe_hostname) = match parse_profile_query(search_query) {
        Ok(result) => result,
        Err(_) => return Ok(vec![]),
    };
    let mut ap_client = ApClient::new(config, db_client).await?;
    ap_client.instance.fetcher_timeout = SEARCH_FETCHER_TIMEOUT;
    let profiles = search_profiles_or_import(
        &ap_client,
        db_client,
        username,
        maybe_hostname,
        resolve,
        limit,
        offset,
    ).await?;
    Ok(profiles)
}

pub async fn search_posts_only(
    current_user: &User,
    db_client: &impl DatabaseClient,
    search_query: &str,
    limit: u16,
    offset: u16,
) -> Result<Vec<Post>, DatabaseError> {
    search_posts(
        db_client,
        search_query,
        current_user.id,
        limit,
        offset,
    ).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_query() {
        let query = ">some text";
        let text = parse_text_query(query).unwrap();
        assert_eq!(text, "some text");
    }

    #[test]
    fn test_parse_profile_query() {
        let query = "@user";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "user");
        assert_eq!(maybe_hostname, None);
    }

    #[test]
    fn test_parse_profile_query_domain_empty() {
        let query = "@user@";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "user");
        assert_eq!(maybe_hostname, None);
    }

    #[test]
    fn test_parse_profile_query_domain_incomplete() {
        let query = "@user@social";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "user");
        assert_eq!(maybe_hostname.as_deref(), Some("social"));
    }

    #[test]
    fn test_parse_profile_query_group() {
        let query = "!group@example.com";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "group");
        assert_eq!(maybe_hostname.as_deref(), Some("example.com"));
    }

    #[test]
    fn test_parse_profile_query_acct_uri() {
        let query = "acct:user@social.example";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "user");
        assert_eq!(maybe_hostname.as_deref(), Some("social.example"));
    }

    #[test]
    fn test_parse_profile_query_idn() {
        let query = "@user_01@☕.example";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "user_01");
        assert_eq!(maybe_hostname.as_deref(), Some("xn--53h.example"));
    }

    #[test]
    fn test_parse_profile_query_invalid_hostname() {
        let query = "@user_01@social%example";
        let error = parse_profile_query(query).unwrap_err();
        assert_eq!(error.to_string(), "invalid hostname");
    }

    #[test]
    fn test_parse_tag_query() {
        let query = "#Activity";
        let tag = parse_tag_query(query).unwrap();

        assert_eq!(tag, "Activity");
    }

    #[test]
    fn test_parse_search_query_single_word() {
        let query = "string";
        let result = parse_search_query(query);
        assert!(matches!(result, SearchQuery::ProfileQuery(_, _)));
    }

    #[test]
    fn test_parse_search_query_text() {
        let query = "some text";
        let result = parse_search_query(query);
        assert!(matches!(result, SearchQuery::Unknown));
    }
}
