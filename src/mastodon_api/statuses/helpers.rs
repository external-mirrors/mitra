use std::collections::HashSet;

use actix_web::{http::Uri, HttpResponse};
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    emojis::types::DbEmoji,
    polls::types::PollResult,
    posts::{
        queries::get_post_by_id,
        helpers::{add_related_posts, add_user_actions, can_link_post},
        types::{Post, Visibility},
    },
    relationships::queries::get_subscribers,
    users::types::User,
};
use mitra_utils::markdown::markdown_lite_to_html;
use mitra_validators::{
    errors::ValidationError,
    polls::clean_poll_option_name,
    posts::clean_local_content,
};

use crate::mastodon_api::{
    errors::MastodonError,
    media_server::ClientMediaServer,
    microsyntax::{
        emojis::{find_emojis, replace_emoji_shortcodes},
        hashtags::{find_hashtags, replace_hashtags},
        links::{find_linked_posts, insert_quote, replace_object_links},
        mentions::{find_mentioned_profiles, replace_mentions},
    },
    pagination::{
        get_last_item,
        get_paginated_response,
        PageSize,
    },
};

use super::types::{
    Status,
    POST_CONTENT_TYPE_HTML,
    POST_CONTENT_TYPE_MARKDOWN,
};

pub struct PostContent {
    pub content: String,
    pub content_source: Option<String>,
    pub mentions: Vec<Uuid>,
    pub hashtags: Vec<String>,
    pub links: Vec<Uuid>,
    pub linked: Vec<Post>,
    pub emojis: Vec<DbEmoji>,
}

async fn parse_microsyntaxes(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    mut content: String,
) -> Result<PostContent, DatabaseError> {
    // Mentions
    let mention_map = find_mentioned_profiles(
        db_client,
        &instance.hostname(),
        &content,
    ).await?;
    content = replace_mentions(
        &mention_map,
        &instance.hostname(),
        &instance.url(),
        &content,
    );
    let mentions = mention_map.values().map(|profile| profile.id).collect();
    // Hashtags
    let hashtags = find_hashtags(&content);
    content = replace_hashtags(
        &instance.url(),
        &content,
        &hashtags,
    );
    // Links
    let link_map = find_linked_posts(
        db_client,
        &instance.url(),
        &content,
    ).await?;
    content = replace_object_links(
        &link_map,
        &content,
    );
    let links = link_map.values().map(|post| post.id).collect();
    let linked = link_map.into_values().collect();
    // Emojis
    let custom_emoji_map = find_emojis(
        db_client,
        &content,
    ).await?;
    content = replace_emoji_shortcodes(&content, &custom_emoji_map);
    let emojis = custom_emoji_map.into_values().collect();
    Ok(PostContent {
        content,
        content_source: None,
        mentions,
        hashtags,
        links,
        linked,
        emojis,
    })
}

pub async fn parse_content(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    content: &str,
    content_type: &str,
    maybe_quote_of_id: Option<Uuid>,
) -> Result<PostContent, MastodonError> {
    let (content_html, maybe_content_source) = match content_type {
        POST_CONTENT_TYPE_HTML => (content.to_owned(), None),
        POST_CONTENT_TYPE_MARKDOWN => {
            let content_html = markdown_lite_to_html(content)
                .map_err(|_| ValidationError("invalid markdown"))?;
            (content_html, Some(content.to_owned()))
        },
        _ => return Err(ValidationError("unsupported content type").into()),
    };
    let mut output = parse_microsyntaxes(
        db_client,
        instance,
        content_html,
    ).await?;
    output.content_source = maybe_content_source;
    if let Some(quote_of_id) = maybe_quote_of_id {
        let quote_of = match get_post_by_id(db_client, quote_of_id).await {
            Ok(post) if can_link_post(&post) => post,
            Ok(_) | Err(DatabaseError::NotFound(_)) => {
                return Err(ValidationError("quoted post does not exist").into());
            },
            Err(other_error) => return Err(other_error.into()),
        };
        if !output.links.contains(&quote_of.id) {
            output.content = insert_quote(
                &instance.url(),
                &output.content,
                &quote_of,
            );
            output.links.insert(0, quote_of.id);
            output.linked.insert(0, quote_of);
        };
    };
    output.content = clean_local_content(&output.content);
    Ok(output)
}

pub async fn parse_poll_options(
    db_client: &impl DatabaseClient,
    poll_options: &[String],
) -> Result<(Vec<PollResult>, Vec<DbEmoji>), DatabaseError> {
    let custom_emoji_map =
        find_emojis(db_client, &poll_options.join(" ")).await?;
    let results = poll_options.iter()
        .map(|name| {
            let name = replace_emoji_shortcodes(name, &custom_emoji_map);
            let name = clean_poll_option_name(&name);
            PollResult::new(&name)
        })
        .collect();
    let emojis = custom_emoji_map.into_values().collect();
    Ok((results, emojis))
}

pub async fn prepare_mentions(
    db_client: &impl DatabaseClient,
    author_id: Uuid,
    visibility: Visibility,
    maybe_in_reply_to: Option<&Post>,
    mut mentions: Vec<Uuid>,
) -> Result<Vec<Uuid>, DatabaseError> {
    // Extend mentions
    if let Some(in_reply_to) = maybe_in_reply_to {
        if in_reply_to.author.id != author_id {
            // Mention the author of the parent post
            mentions.insert(0, in_reply_to.author.id);
        };
    };
    if visibility == Visibility::Subscribers {
        // Mention all subscribers.
        // This makes post accessible only to active subscribers
        // and is required for sending activities to subscribers
        // on other instances.
        let subscribers = get_subscribers(db_client, author_id).await?
            .into_iter().map(|profile| profile.id);
        mentions.extend(subscribers);
    };
    // Remove duplicate mentions (order is preserved)
    let mut mention_set = HashSet::new();
    mentions.retain(|mention| mention_set.insert(*mention));
    Ok(mentions)
}

/// Load related objects and build status for API response
pub async fn build_status(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    media_server: &ClientMediaServer,
    user: Option<&User>,
    mut post: Post,
) -> Result<Status, DatabaseError> {
    add_related_posts(db_client, vec![&mut post]).await?;
    if let Some(user) = user {
        add_user_actions(db_client, user.id, vec![&mut post]).await?;
    };
    let status = Status::from_post(instance_url, media_server, post);
    Ok(status)
}

pub async fn build_status_list(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    media_server: &ClientMediaServer,
    user: Option<&User>,
    mut posts: Vec<Post>,
) -> Result<Vec<Status>, DatabaseError> {
    add_related_posts(db_client, posts.iter_mut().collect()).await?;
    if let Some(user) = user {
        add_user_actions(db_client, user.id, posts.iter_mut().collect()).await?;
    };
    let statuses: Vec<Status> = posts
        .into_iter()
        .map(|post| Status::from_post(instance_url, media_server, post))
        .collect();
    Ok(statuses)
}

#[allow(clippy::too_many_arguments)]
pub async fn get_paginated_status_list(
    db_client: &impl DatabaseClient,
    base_url: &str,
    instance_url: &str,
    media_server: &ClientMediaServer,
    request_uri: &Uri,
    maybe_current_user: Option<&User>,
    posts: Vec<Post>,
    limit: &PageSize,
) -> Result<HttpResponse, DatabaseError> {
    let maybe_last_id = get_last_item(&posts, limit).map(|post| post.id);
    let statuses = build_status_list(
        db_client,
        instance_url,
        media_server,
        maybe_current_user,
        posts,
    ).await?;
    let response = get_paginated_response(
        base_url,
        request_uri,
        statuses,
        maybe_last_id,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        posts::test_utils::create_test_remote_post,
        profiles::test_utils::create_test_remote_profile,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_parse_content_object_link_and_mention() {
        let db_client = &mut create_test_database().await;
        let profile = create_test_remote_profile(
            db_client,
            "test",
            "social.example",
            "https://social.example/users/1",
        ).await;
        let _post = create_test_remote_post(
            db_client,
            profile.id,
            "test",
            "https://social.example/posts/1",
        ).await;
        let instance = Instance::for_test("https://local.example");
        let content_str = "@test@social.example test [[https://social.example/posts/1]].";
        let content = parse_content(
            db_client,
            &instance,
            content_str,
            POST_CONTENT_TYPE_MARKDOWN,
            None,
        ).await.unwrap();
        assert_eq!(
            content.content,
            r#"<p><span class="h-card"><a class="u-url mention" href="https://social.example/users/1" rel="noopener">@test</a></span> test <a href="https://social.example/posts/1" rel="noopener">https://social.example/posts/1</a>.</p>"#,
        );
        assert_eq!(content.mentions.len(), 1);
        assert_eq!(content.links.len(), 1);
    }
}
