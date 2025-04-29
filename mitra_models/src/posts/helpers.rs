use uuid::Uuid;

use crate::{
    bookmarks::queries::find_bookmarked_by_user,
    conversations::queries::is_conversation_participant,
    database::{DatabaseClient, DatabaseError},
    polls::queries::find_votes_by_user,
    profiles::types::DbActorProfile,
    reactions::queries::find_reacted_by_user,
    relationships::{
        queries::has_relationship,
        types::RelationshipType,
    },
    users::types::{Permission, User},
};

use super::queries::{
    get_post_by_id,
    get_related_posts,
    find_reposted_by_user,
};
use super::types::{Post, PostActions, RelatedPosts, Visibility};

pub async fn add_related_posts(
    db_client: &impl DatabaseClient,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    let posts_ids = posts.iter().map(|post| post.id).collect();
    let related = get_related_posts(db_client, posts_ids).await?;
    let get_post = |post_id: Uuid| -> Result<Post, DatabaseError> {
        let post = related.iter()
            .find(|post| post.id == post_id)
            .ok_or(DatabaseError::NotFound("post"))?
            .clone();
        Ok(post)
    };
    for post in posts {
        let mut related_posts = RelatedPosts::default();
        if let Some(in_reply_to_id) = post.in_reply_to_id {
            let in_reply_to = get_post(in_reply_to_id)?;
            related_posts.in_reply_to = Some(Box::new(in_reply_to));
        };
        for linked_id in post.links.clone() {
            let linked = get_post(linked_id)?;
            related_posts.linked.push(linked);
        };
        if let Some(repost_of_id) = post.repost_of_id {
            let mut repost_of = get_post(repost_of_id)?;
            let mut repost_of_related_posts = RelatedPosts::default();
            if let Some(in_reply_to_id) = repost_of.in_reply_to_id {
                let in_reply_to = get_post(in_reply_to_id)?;
                repost_of_related_posts.in_reply_to = Some(Box::new(in_reply_to));
            };
            for linked_id in repost_of.links.clone() {
                let linked = get_post(linked_id)?;
                repost_of_related_posts.linked.push(linked);
            };
            repost_of.related_posts = Some(repost_of_related_posts);
            related_posts.repost_of = Some(Box::new(repost_of));
        };
        post.related_posts = Some(related_posts);
    };
    Ok(())
}

pub async fn add_user_actions(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    // This function can be used without add_related_posts
    let posts_ids: Vec<Uuid> = posts.iter()
        .map(|post| post.id)
        .chain(
            posts.iter()
                .filter_map(|post| {
                    post.related_posts.as_ref()
                        .and_then(|related_posts| related_posts.repost_of.as_ref())
                })
                .map(|post| post.id)
        )
        .collect();
    let reactions = find_reacted_by_user(db_client, user_id, &posts_ids).await?;
    let reposts = find_reposted_by_user(db_client, user_id, &posts_ids).await?;
    let bookmarks = find_bookmarked_by_user(db_client, user_id, &posts_ids).await?;
    let votes = find_votes_by_user(db_client, user_id, &posts_ids).await?;
    let get_actions = |post: &Post| -> PostActions {
        let liked = reactions.iter()
            .any(|(post_id, content)| *post_id == post.id && content.is_none());
        let reacted_with: Vec<_> = reactions.iter()
            .filter(|(post_id, _)| *post_id == post.id)
            .filter_map(|(_, content)| content.clone())
            .collect();
        let reposted = reposts.contains(&post.id);
        let bookmarked = bookmarks.contains(&post.id);
        let voted_for = votes.iter()
            .find(|(post_id, _)| *post_id == post.id)
            .map(|(_, votes)| votes.clone())
            .unwrap_or_default();
        PostActions {
            liked: liked,
            reacted_with: reacted_with,
            reposted: reposted,
            bookmarked: bookmarked,
            voted_for: voted_for,
        }
    };
    for post in posts {
        if let Some(repost_of) = post
            .related_posts.as_mut()
            .and_then(|related_posts| related_posts.repost_of.as_mut())
        {
            let actions = get_actions(repost_of);
            repost_of.actions = Some(actions);
        };
        let actions = get_actions(post);
        post.actions = Some(actions);
    }
    Ok(())
}

// Equivalent to build_visibility_filter
pub async fn can_view_post(
    db_client: &impl DatabaseClient,
    maybe_viewer: Option<&DbActorProfile>,
    post: &Post,
) -> Result<bool, DatabaseError> {
    let is_author = |viewer: &DbActorProfile| post.author.id == viewer.id;
    let is_mentioned = |viewer: &DbActorProfile| {
        post.mentions.iter().any(|profile| profile.id == viewer.id)
    };
    let result = match post.visibility {
        Visibility::Public => true,
        Visibility::Followers => {
            if let Some(viewer) = maybe_viewer {
                let is_following = has_relationship(
                    db_client,
                    viewer.id,
                    post.author.id,
                    RelationshipType::Follow,
                ).await?;
                is_following || is_author(viewer) || is_mentioned(viewer)
            } else {
                false
            }
        },
        Visibility::Subscribers => {
            if let Some(viewer) = maybe_viewer {
                // Can view only if mentioned
                is_author(viewer) || is_mentioned(viewer)
            } else {
                false
            }
        },
        Visibility::Conversation => {
            if let Some(viewer) = maybe_viewer {
                let conversation = post.expect_conversation();
                let is_participant = is_conversation_participant(
                    db_client,
                    viewer.id,
                    conversation.id,
                ).await?;
                is_participant || is_author(viewer) || is_mentioned(viewer)
            } else {
                false
            }
        },
        Visibility::Direct => {
            if let Some(viewer) = maybe_viewer {
                is_author(viewer) || is_mentioned(viewer)
            } else {
                false
            }
        },
    };
    Ok(result)
}

pub fn can_create_post(
    user: &User,
) -> bool {
    user.role.has_permission(Permission::CreatePost)
}

// Equivalent to create_post_links
pub fn can_link_post(post: &Post) -> bool {
    if post.repost_of_id.is_some() {
        // Can't reference reposts
        return false;
    };
    if post.visibility != Visibility::Public {
        // Can't reference non-public posts
        return false;
    };
    true
}

pub async fn get_local_post_by_id(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
) -> Result<Post, DatabaseError> {
    let post = get_post_by_id(db_client, post_id).await?;
    if !post.is_local() {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(post)
}

pub async fn get_post_by_id_for_view(
    db_client: &impl DatabaseClient,
    maybe_viewer: Option<&DbActorProfile>,
    post_id: Uuid,
) -> Result<Post, DatabaseError> {
    let post = get_post_by_id(db_client, post_id).await?;
    if !can_view_post(db_client, maybe_viewer, &post).await? {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(post)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        posts::{
            queries::create_post,
            test_utils::create_test_local_post,
            types::{PostContext, PostCreateData},
        },
        profiles::test_utils::create_test_remote_profile,
        reactions::test_utils::create_test_local_reaction,
        relationships::queries::{follow, subscribe},
        users::{
            test_utils::create_test_user,
            types::{Role, User},
        },
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_add_related_posts_reply() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data = PostCreateData {
            content: "post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, author.id, post_data).await.unwrap();
        let reply_data = PostCreateData {
            context: PostContext::reply_to(&post),
            content: "reply".to_string(),
            ..Default::default()
        };
        let mut reply = create_post(db_client, author.id, reply_data).await.unwrap();
        add_related_posts(db_client, vec![&mut reply]).await.unwrap();
        let related_posts = reply.related_posts.unwrap();
        assert_eq!(related_posts.in_reply_to.unwrap().id, post.id);
        assert_eq!(related_posts.linked.is_empty(), true);
        assert_eq!(related_posts.repost_of.is_none(), true);
    }

    #[tokio::test]
    #[serial]
    async fn test_add_related_posts_repost_of_reply() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data = PostCreateData {
            content: "post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, author.id, post_data).await.unwrap();
        let reply_data = PostCreateData {
            context: PostContext::reply_to(&post),
            content: "reply".to_string(),
            ..Default::default()
        };
        let reply = create_post(db_client, author.id, reply_data).await.unwrap();
        let repost_data = PostCreateData::repost(reply.id, None);
        let mut repost = create_post(db_client, author.id, repost_data).await.unwrap();
        add_related_posts(db_client, vec![&mut repost]).await.unwrap();
        let related_posts = repost.related_posts.unwrap();
        assert_eq!(related_posts.in_reply_to.is_none(), true);
        assert_eq!(related_posts.linked.is_empty(), true);
        let repost_of = related_posts.repost_of.unwrap();
        assert_eq!(repost_of.id, reply.id);
        assert_eq!(
            repost_of.related_posts.unwrap().in_reply_to.unwrap().id,
            post.id,
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_add_user_actions() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let mut post = create_test_local_post(db_client, author.id, "test").await;
        let liker = create_test_user(db_client, "liker").await;

        create_test_local_reaction(db_client, liker.id, post.id, Some("❤️")).await;
        add_user_actions(db_client, liker.id, vec![&mut post]).await.unwrap();
        let actions = post.actions.as_ref().unwrap();
        assert_eq!(actions.liked, false);
        assert_eq!(actions.reacted_with, vec!["❤️".to_string()]);
        assert_eq!(actions.reposted, false);

        create_test_local_reaction(db_client, liker.id, post.id, None).await;
        add_user_actions(db_client, liker.id, vec![&mut post]).await.unwrap();
        let actions = post.actions.as_ref().unwrap();
        assert_eq!(actions.liked, true);
        assert_eq!(actions.reacted_with, vec!["❤️".to_string()]);
        assert_eq!(actions.reposted, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_anonymous() {
        let post = Post {
            visibility: Visibility::Public,
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(db_client, None, &post).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_direct() {
        let user = User::default();
        let post = Post {
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(
            db_client,
            Some(&user.profile),
            &post,
        ).await.unwrap();
        assert_eq!(result, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_direct_author() {
        let user = User::default();
        let post = Post {
            author: user.profile.clone(),
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(
            db_client,
            Some(&user.profile),
            &post,
        ).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_direct_mentioned() {
        let user = User::default();
        let post = Post {
            visibility: Visibility::Direct,
            mentions: vec![user.profile.clone()],
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(
            db_client,
            Some(&user.profile),
            &post,
        ).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_followers_only_anonymous() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let post = Post {
            author: author.profile,
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let result = can_view_post(db_client, None, &post).await.unwrap();
        assert_eq!(result, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_followers_only_follower() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let follower = create_test_user(db_client, "follower").await;
        follow(db_client, follower.id, author.id).await.unwrap();
        let post = Post {
            author: author.profile,
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let result = can_view_post(
            db_client,
            Some(&follower.profile),
            &post,
        ).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_followers_only_remote_follower() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let follower = create_test_remote_profile(
            db_client,
            "follower",
            "remote.example",
            "https://remote.example/actor",
        ).await;
        follow(db_client, follower.id, author.id).await.unwrap();
        let post = Post {
            author: author.profile,
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let result = can_view_post(
            db_client,
            Some(&follower),
            &post,
        ).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_subscribers_only() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let follower = create_test_user(db_client, "follower").await;
        follow(db_client, follower.id, author.id).await.unwrap();
        let subscriber = create_test_user(db_client, "subscriber").await;
        subscribe(db_client, subscriber.id, author.id).await.unwrap();
        let post = Post {
            author: author.profile,
            visibility: Visibility::Subscribers,
            mentions: vec![subscriber.profile.clone()],
            ..Default::default()
        };
        let can_view = can_view_post(db_client, None, &post).await.unwrap();
        assert_eq!(can_view, false);
        let can_view = can_view_post(
            db_client,
            Some(&follower.profile),
            &post,
        ).await.unwrap();
        assert_eq!(can_view, false);
        let can_view = can_view_post(
            db_client,
            Some(&subscriber.profile),
            &post,
        ).await.unwrap();
        assert_eq!(can_view, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_conversation() {
        let db_client = &mut create_test_database().await;
        let root_author = create_test_user(db_client, "op").await;
        let root_follower = create_test_user(db_client, "root_follower").await;
        follow(db_client, root_follower.id, root_author.id).await.unwrap();
        let root_data = PostCreateData {
            context: PostContext::Top { audience: None },
            content: "root".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let root = create_post(db_client, root_author.id, root_data).await.unwrap();
        let author = create_test_user(db_client, "author").await;
        let author_follower = create_test_user(db_client, "author_follower").await;
        follow(db_client, author_follower.id, author.id).await.unwrap();
        let post = Post {
            author: author.profile.clone(),
            conversation: root.conversation.clone(),
            in_reply_to_id: Some(root.id),
            visibility: Visibility::Conversation,
            ..Default::default()
        };
        let can_view = can_view_post(db_client, None, &post).await.unwrap();
        assert_eq!(can_view, false);
        let can_view = can_view_post(
            db_client,
            Some(&author.profile),
            &post,
        ).await.unwrap();
        assert_eq!(can_view, true);
        let can_view = can_view_post(
            db_client,
            Some(&root_follower.profile),
            &post,
        ).await.unwrap();
        assert_eq!(can_view, true);
        let can_view = can_view_post(
            db_client,
            Some(&author_follower.profile),
            &post,
        ).await.unwrap();
        assert_eq!(can_view, false);
    }

    #[test]
    fn test_can_create_post() {
        let mut user = User {
            role: Role::NormalUser,
            ..Default::default()
        };
        assert_eq!(can_create_post(&user), true);
        user.role = Role::ReadOnlyUser;
        assert_eq!(can_create_post(&user), false);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_post_by_id_for_view() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data = PostCreateData {
            content: "post".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post = create_post(db_client, author.id, post_data).await.unwrap();
        // View as guest
        let error = get_post_by_id_for_view(
            db_client,
            None,
            post.id,
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "post not found");
    }
}
