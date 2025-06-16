use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::attachments::{
    queries::set_attachment_ipfs_cid,
    types::DbMediaAttachment,
};
use crate::conversations::{
    queries::{
        create_conversation,
        get_conversation,
    },
};
use crate::database::{
    catch_unique_violation,
    query_macro::query,
    DatabaseClient,
    DatabaseError,
    DatabaseTypeError,
};
use crate::emojis::types::DbEmoji;
use crate::media::types::DeletionQueue;
use crate::notifications::helpers::{
    create_mention_notification,
    create_reply_notification,
    create_repost_notification,
};
use crate::polls::queries::{create_poll, reset_votes, update_poll};
use crate::profiles::{
    queries::update_post_count,
    types::DbActorProfile,
};
use crate::relationships::types::RelationshipType;

use super::types::{
    DbLanguage,
    DbPost,
    Post,
    PostContext,
    PostCreateData,
    PostReaction,
    PostUpdateData,
    Repost,
    Visibility,
};

async fn create_post_attachments(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    author_id: Uuid,
    attachments: Vec<Uuid>,
) -> Result<Vec<DbMediaAttachment>, DatabaseError> {
    let attachments_rows = db_client.query(
        "
        UPDATE media_attachment
        SET post_id = $1
        WHERE owner_id = $2 AND id = ANY($3)
        RETURNING media_attachment
        ",
        &[&post_id, &author_id, &attachments],
    ).await?;
    if attachments_rows.len() != attachments.len() {
        // Some attachments were not found
        return Err(DatabaseError::NotFound("attachment"));
    };
    let mut attachments: Vec<DbMediaAttachment> = attachments_rows.iter()
        .map(|row| row.try_get("media_attachment"))
        .collect::<Result<_, _>>()?;
    attachments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(attachments)
}

async fn create_post_mentions(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    mentions: Vec<Uuid>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let mentions_rows = db_client.query(
        "
        INSERT INTO post_mention (post_id, profile_id)
        SELECT $1, profile_id
        FROM unnest($2::uuid[]) WITH ORDINALITY AS mention(profile_id, rank)
        JOIN actor_profile ON profile_id = actor_profile.id
        ORDER BY rank
        RETURNING (
            SELECT actor_profile FROM actor_profile
            WHERE actor_profile.id = profile_id
        ) AS actor_profile
        ",
        &[&post_id, &mentions],
    ).await?;
    if mentions_rows.len() != mentions.len() {
        // Some profiles were not found
        return Err(DatabaseError::NotFound("profile"));
    };
    let profiles = mentions_rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

async fn create_post_tags(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    tags: Vec<String>,
) -> Result<Vec<String>, DatabaseError> {
    db_client.execute(
        "
        INSERT INTO tag (tag_name)
        SELECT unnest($1::text[])
        ON CONFLICT (tag_name) DO NOTHING
        ",
        &[&tags],
    ).await?;
    let tags_rows = db_client.query(
        "
        INSERT INTO post_tag (post_id, tag_id)
        SELECT $1, tag.id FROM tag WHERE tag_name = ANY($2)
        RETURNING (SELECT tag_name FROM tag WHERE tag.id = tag_id)
        ",
        &[&post_id, &tags],
    ).await?;
    if tags_rows.len() != tags.len() {
        return Err(DatabaseError::NotFound("tag"));
    };
    let tags = tags_rows.iter()
        .map(|row| row.try_get("tag_name"))
        .collect::<Result<_, _>>()?;
    Ok(tags)
}

async fn create_post_links(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    links: Vec<Uuid>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let links_rows = db_client.query(
        "
        INSERT INTO post_link (source_id, target_id)
        SELECT $1, post.id FROM post
        WHERE
            post.id = ANY($2)
            AND post.repost_of_id IS NULL
            AND post.visibility = $3
        RETURNING target_id
        ",
        &[&post_id, &links, &Visibility::Public],
    ).await?;
    if links_rows.len() != links.len() {
        return Err(DatabaseError::NotFound("post"));
    };
    let links = links_rows.iter()
        .map(|row| row.try_get("target_id"))
        .collect::<Result<_, _>>()?;
    Ok(links)
}

async fn create_post_emojis(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    emojis: Vec<Uuid>,
) -> Result<Vec<DbEmoji>, DatabaseError> {
    let emojis_rows = db_client.query(
        "
        INSERT INTO post_emoji (post_id, emoji_id)
        SELECT $1, emoji.id FROM emoji WHERE id = ANY($2)
        RETURNING (
            SELECT emoji FROM emoji
            WHERE emoji.id = emoji_id
        )
        ",
        &[&post_id, &emojis],
    ).await?;
    if emojis_rows.len() != emojis.len() {
        return Err(DatabaseError::NotFound("emoji"));
    };
    let emojis = emojis_rows.iter()
        .map(|row| row.try_get("emoji"))
        .collect::<Result<_, _>>()?;
    Ok(emojis)
}

pub async fn get_post_reactions(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
) -> Result<Vec<PostReaction>, DatabaseError> {
    let statement = format!(
        "
        SELECT {related_reactions}
        FROM post
        WHERE post.id = $1
        ",
        related_reactions=RELATED_REACTIONS,
    );
    let maybe_row = db_client.query_opt(
        &statement,
        &[&post_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let reactions: Vec<PostReaction> = row.try_get("reactions")?;
    Ok(reactions)
}

pub async fn create_post(
    db_client: &mut impl DatabaseClient,
    author_id: Uuid,
    post_data: PostCreateData,
) -> Result<Post, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let post_id = post_data.id.unwrap_or_else(generate_ulid);

    // Create or find existing conversation
    let maybe_conversation = match post_data.context {
        PostContext::Top { ref audience } => {
            let conversation = create_conversation(
                &transaction,
                post_id,
                audience.as_deref(),
            ).await?;
            Some(conversation)
        },
        PostContext::Reply { conversation_id, .. } => {
            let conversation =
                get_conversation(&transaction, conversation_id).await?;
            Some(conversation)
        },
        PostContext::Repost { .. } => None,
    };

    // Create post
    let insert_statement = format!(
        "
        INSERT INTO post (
            id,
            author_id,
            content,
            content_source,
            language,
            conversation_id,
            in_reply_to_id,
            repost_of_id,
            visibility,
            is_sensitive,
            url,
            object_id,
            created_at
        )
        SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
        WHERE
        -- don't allow replies to reposts
        NOT EXISTS (
            SELECT 1 FROM post
            WHERE post.id = $7 AND post.repost_of_id IS NOT NULL
        )
        -- don't allow reposts of non-public posts
        AND NOT EXISTS (
            SELECT 1 FROM post
            WHERE post.id = $8 AND (
                post.repost_of_id IS NOT NULL
                OR post.visibility != {visibility_public}
            )
        )
        RETURNING post
        ",
        visibility_public=i16::from(Visibility::Public),
    );
    let maybe_post_row = transaction.query_opt(
        &insert_statement,
        &[
            &post_id,
            &author_id,
            &post_data.content,
            &post_data.content_source,
            &post_data.language.map(DbLanguage::new),
            &maybe_conversation.as_ref().map(|conversation| conversation.id),
            &post_data.context.in_reply_to_id(),
            &post_data.context.repost_of_id(),
            &post_data.visibility,
            &post_data.is_sensitive,
            &post_data.url,
            &post_data.object_id,
            &post_data.created_at,
        ],
    ).await.map_err(catch_unique_violation("post"))?;
    // Return NotFound error if reply/repost is not allowed
    let post_row = maybe_post_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = post_row.try_get("post")?;

    // Create related objects
    let db_attachments = create_post_attachments(
        &transaction,
        db_post.id,
        db_post.author_id,
        post_data.attachments,
    ).await?;
    let db_mentions = create_post_mentions(
        &transaction,
        db_post.id,
        post_data.mentions,
    ).await?;
    let db_tags = create_post_tags(
        &transaction,
        db_post.id,
        post_data.tags,
    ).await?;
    let db_links = create_post_links(
        &transaction,
        db_post.id,
        post_data.links,
    ).await?;
    let db_emojis = create_post_emojis(
        &transaction,
        db_post.id,
        post_data.emojis,
    ).await?;
    let maybe_poll = if let Some(poll_data) = post_data.poll {
        let poll = create_poll(
            &transaction,
            db_post.id,
            poll_data,
        ).await?;
        Some(poll)
    } else {
        None
    };

    // Update counters
    let author = update_post_count(&transaction, db_post.author_id, 1).await?;
    let mut notified_users = vec![];
    if let Some(in_reply_to_id) = db_post.in_reply_to_id {
        update_reply_count(&transaction, in_reply_to_id, 1).await?;
        let in_reply_to_author = get_post_author(&transaction, in_reply_to_id).await?;
        if in_reply_to_author.is_local() &&
            in_reply_to_author.id != db_post.author_id
        {
            create_reply_notification(
                &transaction,
                db_post.author_id,
                in_reply_to_author.id,
                db_post.id,
            ).await?;
            notified_users.push(in_reply_to_author.id);
        };
    };
    // Notify reposted
    if let Some(repost_of_id) = db_post.repost_of_id {
        update_repost_count(&transaction, repost_of_id, 1).await?;
        let repost_of_author = get_post_author(&transaction, repost_of_id).await?;
        if repost_of_author.is_local() &&
            // Don't notify themselves that they reposted their post
            repost_of_author.id != db_post.author_id &&
            !notified_users.contains(&repost_of_author.id)
        {
            create_repost_notification(
                &transaction,
                db_post.author_id,
                repost_of_author.id,
                repost_of_id,
            ).await?;
            notified_users.push(repost_of_author.id);
        };
    };
    // Notify mentioned users
    for profile in db_mentions.iter() {
        if profile.is_local() &&
            profile.id != db_post.author_id &&
            // Don't send mention notification to the author of parent post
            // or to the author of reposted post
            !notified_users.contains(&profile.id)
        {
            create_mention_notification(
                &transaction,
                db_post.author_id,
                profile.id,
                db_post.id,
            ).await?;
        };
    };
    // Construct post object
    let post = Post::new(
        db_post,
        author,
        maybe_conversation,
        maybe_poll,
        db_attachments,
        db_mentions,
        db_tags,
        db_links,
        db_emojis,
        vec![],
    )?;
    transaction.commit().await?;
    Ok(post)
}

pub async fn update_post(
    db_client: &mut impl DatabaseClient,
    post_id: Uuid,
    post_data: PostUpdateData,
) -> Result<(Post, DeletionQueue), DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Reposts and immutable posts can't be updated
    let maybe_row = transaction.query_opt(
        "
        UPDATE post
        SET
            content = $1,
            content_source = $2,
            language = $3,
            is_sensitive = $4,
            url = $5,
            updated_at = $6
        WHERE id = $7
            AND repost_of_id IS NULL
            AND ipfs_cid IS NULL
        RETURNING post
        ",
        &[
            &post_data.content,
            &post_data.content_source,
            &post_data.language.map(DbLanguage::new),
            &post_data.is_sensitive,
            &post_data.url,
            &post_data.updated_at,
            &post_id,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = row.try_get("post")?;

    // Get conversation details
    let conversation = get_conversation(
        &transaction,
        db_post.conversation_id.expect("should not be a repost"),
    ).await?;

    // Delete and re-create related objects
    let detached_media_rows = transaction.query(
        "
        DELETE FROM media_attachment
        WHERE post_id = $1 AND id <> ALL($2)
        RETURNING file_name, ipfs_cid
        ",
        &[&db_post.id, &post_data.attachments],
    ).await?;
    let mut detached_files = vec![];
    let mut detached_ipfs_objects = vec![];
    for row in detached_media_rows {
        let file_name = row.try_get("file_name")?;
        detached_files.push(file_name);
        let maybe_ipfs_cid: Option<String> = row.try_get("ipfs_cid")?;
        if let Some(ipfs_cid) = maybe_ipfs_cid {
            detached_ipfs_objects.push(ipfs_cid);
        };
    };
    let old_mentions_rows = transaction.query(
        "
        DELETE FROM post_mention WHERE post_id = $1
        RETURNING profile_id
        ",
        &[&db_post.id],
    ).await?;
    let old_mentions: Vec<Uuid> = old_mentions_rows.iter()
        .map(|row| row.try_get("profile_id"))
        .collect::<Result<_, _>>()?;
    transaction.execute(
        "DELETE FROM post_tag WHERE post_id = $1",
        &[&db_post.id],
    ).await?;
    transaction.execute(
        "DELETE FROM post_link WHERE source_id = $1",
        &[&db_post.id],
    ).await?;
    transaction.execute(
        "DELETE FROM post_emoji WHERE post_id = $1",
        &[&db_post.id],
    ).await?;
    let db_attachments = create_post_attachments(
        &transaction,
        db_post.id,
        db_post.author_id,
        post_data.attachments,
    ).await?;
    let db_mentions = create_post_mentions(
        &transaction,
        db_post.id,
        post_data.mentions,
    ).await?;
    let db_tags = create_post_tags(
        &transaction,
        db_post.id,
        post_data.tags,
    ).await?;
    let db_links = create_post_links(
        &transaction,
        db_post.id,
        post_data.links,
    ).await?;
    let db_emojis = create_post_emojis(
        &transaction,
        db_post.id,
        post_data.emojis,
    ).await?;
    let db_reactions =
        get_post_reactions(&transaction, db_post.id).await?;
    let maybe_poll = if let Some(poll_data) = post_data.poll {
        let (poll, options_changed) = update_poll(
            &transaction,
            db_post.id,
            poll_data,
        ).await?;
        if options_changed {
            reset_votes(&transaction, poll.id).await?;
        };
        Some(poll)
    } else {
        None
    };

    // Create notifications
    for profile in db_mentions.iter() {
        if profile.is_local() &&
            profile.id != db_post.author_id &&
            !old_mentions.contains(&profile.id)
        {
            create_mention_notification(
                &transaction,
                db_post.author_id,
                profile.id,
                db_post.id,
            ).await?;
        };
    };

    // Construct post object
    let author = get_post_author(&transaction, db_post.id).await?;
    let post = Post::new(
        db_post,
        author,
        Some(conversation),
        maybe_poll,
        db_attachments,
        db_mentions,
        db_tags,
        db_links,
        db_emojis,
        db_reactions,
    )?;
    transaction.commit().await?;
    let deletion_queue = DeletionQueue {
        files: detached_files,
        ipfs_objects: detached_ipfs_objects,
    };
    Ok((post, deletion_queue))
}

const RELATED_CONVERSATION: &str = "
    (
        SELECT conversation
        FROM conversation
        WHERE conversation.id = post.conversation_id
    ) AS conversation";

const RELATED_POLL: &str = "
    (
        SELECT poll
        FROM poll
        WHERE poll.id = post.id
    ) AS poll";

const RELATED_ATTACHMENTS: &str = "
    ARRAY(
        SELECT media_attachment
        FROM media_attachment WHERE post_id = post.id
        ORDER BY media_attachment.created_at
    ) AS attachments";

const RELATED_MENTIONS: &str = "
    ARRAY(
        SELECT actor_profile
        FROM post_mention
        JOIN actor_profile ON post_mention.profile_id = actor_profile.id
        WHERE post_id = post.id
        ORDER BY post_mention.id
    ) AS mentions";

const RELATED_TAGS: &str = "
    ARRAY(
        SELECT tag.tag_name FROM tag
        JOIN post_tag ON post_tag.tag_id = tag.id
        WHERE post_tag.post_id = post.id
    ) AS tags";

const RELATED_LINKS: &str = "
    ARRAY(
        SELECT post_link.target_id FROM post_link
        WHERE post_link.source_id = post.id
    ) AS links";

const RELATED_EMOJIS: &str = "
    ARRAY(
        SELECT emoji
        FROM post_emoji
        JOIN emoji ON post_emoji.emoji_id = emoji.id
        WHERE post_emoji.post_id = post.id
    ) AS emojis";

const RELATED_REACTIONS: &str = "
    ARRAY(
        SELECT
            json_build_object(
                'content', post_reaction.content,
                'emoji', (array_agg(emoji))[1],
                'count', count(post_reaction)
            )
        FROM post_reaction
        LEFT JOIN emoji
        ON post_reaction.emoji_id = emoji.id
        WHERE post_reaction.post_id = post.id
        GROUP BY post_reaction.content
    ) AS reactions";

pub(crate) fn post_subqueries() -> String {
    [
        RELATED_CONVERSATION,
        RELATED_POLL,
        RELATED_ATTACHMENTS,
        RELATED_MENTIONS,
        RELATED_TAGS,
        RELATED_LINKS,
        RELATED_EMOJIS,
        RELATED_REACTIONS,
    ].join(",")
}

fn build_visibility_filter() -> String {
    format!(
        "(
            post.author_id = $current_user_id
            OR post.visibility = {visibility_public}
            -- covers direct messages and subscribers-only posts
            OR EXISTS (
                SELECT 1 FROM post_mention
                WHERE post_id = post.id AND profile_id = $current_user_id
            )
            OR EXISTS (
                SELECT 1 FROM post AS repost_of
                WHERE
                    post.repost_of_id = repost_of.id
                    AND repost_of.author_id = $current_user_id
            )
            OR EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = $current_user_id
                    AND target_id = post.author_id
                    AND (
                        post.visibility = {visibility_followers}
                        AND relationship_type = {relationship_follow}
                        OR post.visibility = {visibility_subscribers}
                        AND relationship_type = {relationship_subscription}
                    )
            )
            OR post.visibility = {visibility_conversation} AND EXISTS (
                SELECT 1
                FROM conversation
                JOIN post AS root ON conversation.root_id = root.id
                WHERE
                    conversation.id = post.conversation_id
                    AND (
                        root.author_id = $current_user_id
                        OR EXISTS (
                            SELECT 1 FROM relationship
                            WHERE
                                source_id = $current_user_id
                                AND target_id = root.author_id
                                AND (
                                    root.visibility = {visibility_followers}
                                    AND relationship_type = {relationship_follow}
                                    OR root.visibility = {visibility_subscribers}
                                    AND relationship_type = {relationship_subscription}
                                )
                        )
                    )
            )
        )",
        visibility_public=i16::from(Visibility::Public),
        visibility_followers=i16::from(Visibility::Followers),
        visibility_subscribers=i16::from(Visibility::Subscribers),
        visibility_conversation=i16::from(Visibility::Conversation),
        relationship_follow=i16::from(RelationshipType::Follow),
        relationship_subscription=i16::from(RelationshipType::Subscription),
    )
}

fn build_mute_filter() -> String {
    format!(
        "(
            NOT EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = $current_user_id
                    AND target_id = post.author_id
                    AND relationship_type = {relationship_mute}
            )
            AND NOT EXISTS (
                SELECT 1
                FROM post AS repost_of, relationship
                WHERE
                    repost_of.id = post.repost_of_id
                    AND source_id = $current_user_id
                    AND target_id = repost_of.author_id
                    AND relationship_type = {relationship_mute}
            )
        )",
        relationship_mute=i16::from(RelationshipType::Mute),
    )
}

pub async fn get_home_timeline(
    db_client: &impl DatabaseClient,
    current_user_id: Uuid,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    // Select posts from follows, subscriptions,
    // posts where current user is mentioned
    // and user's own posts.
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            (
                post.author_id = $current_user_id
                OR (
                    -- is following or subscribed to the post author
                    EXISTS (
                        SELECT 1 FROM relationship
                        WHERE
                            source_id = $current_user_id
                            AND target_id = post.author_id
                            AND relationship_type IN ({relationship_follow}, {relationship_subscription})
                    )
                    AND (
                        -- show posts
                        post.repost_of_id IS NULL
                        -- show reposts if they are not hidden
                        OR NOT EXISTS (
                            SELECT 1 FROM relationship
                            WHERE
                                source_id = $current_user_id
                                AND target_id = post.author_id
                                AND relationship_type = {relationship_hide_reposts}
                        )
                        -- show reposts of current user's posts
                        OR EXISTS (
                            SELECT 1 FROM post AS repost_of
                            WHERE repost_of.id = post.repost_of_id
                                AND repost_of.author_id = $current_user_id
                        )
                    )
                    AND (
                        -- show posts (top-level)
                        post.in_reply_to_id IS NULL
                        -- show replies if they are not hidden
                        OR NOT EXISTS (
                            SELECT 1 FROM relationship
                            WHERE
                                source_id = $current_user_id
                                AND target_id = post.author_id
                                AND relationship_type = {relationship_hide_replies}
                        )
                        -- show replies to current user's posts
                        OR EXISTS (
                            SELECT 1 FROM post AS in_reply_to
                            WHERE
                                in_reply_to.id = post.in_reply_to_id
                                AND in_reply_to.author_id = $current_user_id
                        )
                    )
                    -- exlclude authors that are displayed in custom feeds
                    AND NOT EXISTS (
                        SELECT 1 FROM custom_feed_source
                        JOIN custom_feed ON custom_feed.id = custom_feed_source.feed_id
                        WHERE custom_feed.owner_id = $current_user_id
                            AND custom_feed_source.source_id = post.author_id
                    )
                )
                OR EXISTS (
                    SELECT 1 FROM post_mention
                    WHERE post_id = post.id AND profile_id = $current_user_id
                )
            )
            -- author is not muted
            AND {mute_filter}
            AND {visibility_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
        relationship_follow=i16::from(RelationshipType::Follow),
        relationship_subscription=i16::from(RelationshipType::Subscription),
        relationship_hide_reposts=i16::from(RelationshipType::HideReposts),
        relationship_hide_replies=i16::from(RelationshipType::HideReplies),
        mute_filter=build_mute_filter(),
        visibility_filter=build_visibility_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_public_timeline(
    db_client: &impl DatabaseClient,
    current_user_id: Option<Uuid>,
    only_local: bool,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let mut filter = "".to_owned();
    if only_local {
        filter += "(actor_profile.user_id IS NOT NULL
            OR actor_profile.portable_user_id IS NOT NULL) AND";
    };
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            {filter}
            post.visibility = {visibility_public}
            AND post.repost_of_id IS NULL
            AND {mute_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
        filter=filter,
        visibility_public=i16::from(Visibility::Public),
        mute_filter=build_mute_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_direct_timeline(
    db_client: &impl DatabaseClient,
    current_user_id: Uuid,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            (
                post.author_id = $current_user_id
                OR EXISTS (
                    SELECT 1 FROM post_mention
                    WHERE post_id = post.id AND profile_id = $current_user_id
                )
            )
            AND post.visibility = {visibility_direct}
            AND {mute_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
        visibility_direct=i16::from(Visibility::Direct),
        mute_filter=build_mute_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub(super) async fn get_related_posts(
    db_client: &impl DatabaseClient,
    posts_ids: Vec<Uuid>,
) -> Result<Vec<Post>, DatabaseError> {
    // WARNING: read permissions are not checked here.
    // Replies: scope widening is not allowed for local posts,
    // but allowed for remote posts.
    // Reposts: reposts of non-public posts are not allowed.
    // Links: links to non-public posts are not allowed.
    let statement = format!(
        "
        WITH post_ids AS (SELECT unnest($1::uuid[]) AS post_id)
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id IN (
            SELECT post.in_reply_to_id
            FROM post WHERE post.id = ANY(SELECT post_id FROM post_ids)
            UNION ALL
            SELECT post.repost_of_id
            FROM post WHERE post.id = ANY(SELECT post_id FROM post_ids)
            UNION ALL
            SELECT post_link.target_id
            FROM post_link WHERE post_link.source_id = ANY(SELECT post_id FROM post_ids)
            UNION ALL
            SELECT repost_of.in_reply_to_id
            FROM post AS repost_of JOIN post ON (post.repost_of_id = repost_of.id)
            WHERE post.id = ANY(SELECT post_id FROM post_ids)
            UNION ALL
            SELECT post_link.target_id
            FROM post_link JOIN post ON (post.repost_of_id = post_link.source_id)
            WHERE post.id = ANY(SELECT post_id FROM post_ids)
        )
        ",
        post_subqueries=post_subqueries(),
    );
    let rows = db_client.query(
        &statement,
        &[&posts_ids],
    ).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

#[allow(clippy::too_many_arguments)]
pub async fn get_posts_by_author(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    current_user_id: Option<Uuid>,
    include_replies: bool,
    include_reposts: bool,
    only_pinned: bool,
    only_media: bool,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let mut condition = format!(
        "post.author_id = $profile_id
        AND {visibility_filter}
        AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)",
        visibility_filter=build_visibility_filter(),
    );
    if !include_replies {
        condition.push_str(" AND post.in_reply_to_id IS NULL");
    };
    if !include_reposts {
        condition.push_str(" AND post.repost_of_id IS NULL");
    };
    if only_pinned {
        condition.push_str(" AND post.is_pinned IS TRUE");
    };
    if only_media {
        condition.push_str(
            " AND EXISTS(
                SELECT 1 FROM media_attachment
                WHERE post_id = post.id)");
    };
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE {condition}
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
        condition=condition,
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        profile_id=profile_id,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts_by_tag(
    db_client: &impl DatabaseClient,
    tag_name: &str,
    current_user_id: Option<Uuid>,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let tag_name = tag_name.to_lowercase();
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            EXISTS (
                SELECT 1 FROM post_tag JOIN tag ON post_tag.tag_id = tag.id
                WHERE post_tag.post_id = post.id AND tag.tag_name = $tag_name
            )
            AND {visibility_filter}
            AND {mute_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
        visibility_filter=build_visibility_filter(),
        mute_filter=build_mute_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        tag_name=tag_name,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_custom_feed_timeline(
    db_client: &impl DatabaseClient,
    feed_id: i32,
    current_user_id: Uuid,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    // show_replies / show_reposts settings are ignored
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            EXISTS (
                SELECT 1
                FROM custom_feed
                JOIN custom_feed_source
                ON custom_feed.id = custom_feed_source.feed_id
                WHERE
                    custom_feed.id = $feed_id
                    AND post.author_id = custom_feed_source.source_id
            )
            AND (
                -- show posts
                post.repost_of_id IS NULL
                -- show reposts if they are not hidden
                OR NOT EXISTS (
                    SELECT 1 FROM relationship
                    WHERE
                        source_id = $current_user_id
                        AND target_id = post.author_id
                        AND relationship_type = {relationship_hide_reposts}
                )
            )
            AND (
                -- show posts (top-level)
                post.in_reply_to_id IS NULL
                -- show replies if they are not hidden
                OR NOT EXISTS (
                    SELECT 1 FROM relationship
                    WHERE
                        source_id = $current_user_id
                        AND target_id = post.author_id
                        AND relationship_type = {relationship_hide_replies}
                )
            )
            AND {visibility_filter}
            AND {mute_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        post_subqueries=post_subqueries(),
        relationship_hide_reposts=i16::from(RelationshipType::HideReposts),
        relationship_hide_replies=i16::from(RelationshipType::HideReplies),
        visibility_filter=build_visibility_filter(),
        mute_filter=build_mute_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        feed_id=feed_id,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

/// Get a single post (not a repost)
pub async fn get_post_by_id(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
            AND post.repost_of_id IS NULL
        ",
        post_subqueries=post_subqueries(),
    );
    let maybe_row = db_client.query_opt(
        &statement,
        &[&post_id],
    ).await?;
    let post = match maybe_row {
        Some(row) => Post::try_from(&row)?,
        None => return Err(DatabaseError::NotFound("post")),
    };
    Ok(post)
}

/// Given a post ID, finds all items in thread.
/// Results are sorted by tree path.
pub async fn get_thread(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    current_user_id: Option<Uuid>,
) -> Result<Vec<Post>, DatabaseError> {
    // TODO: limit recursion depth
    let statement = format!(
        "
        WITH RECURSIVE
        ancestors (id, in_reply_to_id) AS (
            SELECT post.id, post.in_reply_to_id FROM post
            WHERE post.id = $post_id
                AND post.repost_of_id IS NULL
                AND {visibility_filter}
            UNION ALL
            SELECT post.id, post.in_reply_to_id FROM post
            JOIN ancestors ON post.id = ancestors.in_reply_to_id
        ),
        thread (id, path) AS (
            SELECT ancestors.id, ARRAY[ancestors.id] FROM ancestors
            WHERE ancestors.in_reply_to_id IS NULL
            UNION
            SELECT post.id, array_append(thread.path, post.id) FROM post
            JOIN thread ON post.in_reply_to_id = thread.id
        )
        SELECT
            post, actor_profile,
            {post_subqueries},
            EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = $current_user_id
                    AND target_id = post.author_id
                    AND relationship_type = {relationship_mute}
            ) AS is_author_muted
        FROM post
        JOIN thread ON post.id = thread.id
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            {visibility_filter}
        ORDER BY thread.path
        ",
        post_subqueries=post_subqueries(),
        relationship_mute=i16::from(RelationshipType::Mute),
        visibility_filter=build_visibility_filter(),
    );
    let query = query!(
        &statement,
        post_id=post_id,
        current_user_id=current_user_id,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let mut posts = vec![];
    let mut hidden_posts = vec![];
    for row in rows {
        let mut post = Post::try_from(&row)?;
        if let Some(ref in_reply_to_id) = post.in_reply_to_id {
            if hidden_posts.contains(in_reply_to_id) {
                post.parent_visible = false;
            };
        };
        let is_author_muted = row.try_get("is_author_muted")?;
        if is_author_muted {
            hidden_posts.push(post.id);
            // Don't include muted post
            continue;
        };
        posts.push(post);
    };
    if posts.is_empty() {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(posts)
}

/// Returns all posts in a conversation
pub async fn get_conversation_items(
    db_client: &impl DatabaseClient,
    conversation_id: Uuid,
    current_user_id: Option<Uuid>,
) -> Result<(Post, Vec<Post>), DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            conversation_id = $conversation_id
            AND {visibility_filter}
        ORDER BY post.id
        ",
        post_subqueries=post_subqueries(),
        visibility_filter=build_visibility_filter(),
    );
    let query = query!(
        &statement,
        conversation_id=conversation_id,
        current_user_id=current_user_id,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    let root = match &posts[..] {
        [] => return Err(DatabaseError::NotFound("conversation")),
        [root, ..] => {
            if !root.conversation.as_ref()
                .is_some_and(|conversation| conversation.root_id == root.id)
            {
                // Consistency check: unexpected root
                return Err(DatabaseTypeError.into());
            };
            root
        },
    };
    Ok((root.clone(), posts))
}

/// Returns actors participating in a conversation (a chain of replies)
pub async fn get_conversation_participants(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        WITH RECURSIVE ancestors (author_id, in_reply_to_id) AS (
            SELECT post.author_id, post.in_reply_to_id
            FROM post
            WHERE post.id = $1 AND post.repost_of_id IS NULL
            UNION
            SELECT post.author_id, post.in_reply_to_id
            FROM post
            JOIN ancestors ON post.id = ancestors.in_reply_to_id
        )
        SELECT actor_profile
        FROM ancestors
        JOIN actor_profile ON ancestors.author_id = actor_profile.id
        ",
        &[&post_id],
    ).await?;
    let profiles: Vec<DbActorProfile> = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    if profiles.is_empty() {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(profiles)
}

pub async fn get_remote_post_by_object_id(
    db_client: &impl DatabaseClient,
    object_id: &str,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.object_id = $1 AND post.repost_of_id IS NULL
        ",
        post_subqueries=post_subqueries(),
    );
    let maybe_row = db_client.query_opt(
        &statement,
        &[&object_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let post = Post::try_from(&row)?;
    Ok(post)
}

pub async fn get_remote_repost_by_activity_id(
    db_client: &impl DatabaseClient,
    activity_id: &str,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.object_id = $1 AND post.repost_of_id IS NOT NULL
        ",
        post_subqueries=post_subqueries(),
    );
    let maybe_row = db_client.query_opt(
        &statement,
        &[&activity_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let repost = Post::try_from(&row)?;
    Ok(repost)
}

pub async fn set_pinned_flag(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    is_pinned: bool,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET is_pinned = $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
        &[&is_pinned, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn update_reply_count(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET reply_count = reply_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
        &[&change, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(())
}

pub async fn update_reaction_count(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET reaction_count = reaction_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
        &[&change, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn update_repost_count(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET repost_count = repost_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
        &[&change, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn set_post_ipfs_cid(
    db_client: &mut impl DatabaseClient,
    post_id: Uuid,
    ipfs_cid: &str,
    attachments: Vec<(Uuid, String)>,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let updated_count = transaction.execute(
        "
        UPDATE post
        SET ipfs_cid = $1
        WHERE id = $2
            AND repost_of_id IS NULL
            AND ipfs_cid IS NULL
        ",
        &[&ipfs_cid, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    for (attachment_id, cid) in attachments {
        set_attachment_ipfs_cid(&transaction, attachment_id, &cid).await?;
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn get_post_author(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT actor_profile
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
        ",
        &[&post_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let author: DbActorProfile = row.try_get("actor_profile")?;
    Ok(author)
}

/// Finds repost of a given post
pub async fn get_repost_by_author(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    author_id: Uuid,
) -> Result<Repost, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT post
        FROM post
        WHERE post.repost_of_id = $1 AND post.author_id = $2
        ",
        &[&post_id, &author_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let repost = Repost::try_from(&row)?;
    Ok(repost)
}

/// Returns reposts of a given post
pub async fn get_post_reposts(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    current_user_id: Option<Uuid>,
    max_repost_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<(Uuid, DbActorProfile)>, DatabaseError> {
    let statement = format!(
        "
        SELECT repost.id, actor_profile
        FROM (
            SELECT * FROM post
            WHERE {visibility_filter}
        ) AS repost
        JOIN actor_profile ON repost.author_id = actor_profile.id
        WHERE
            repost.repost_of_id = $post_id
            AND ($max_repost_id::uuid IS NULL OR repost.id < $max_repost_id)
        ORDER BY repost.id DESC
        LIMIT $limit
        ",
        visibility_filter=build_visibility_filter(),
    );
    let limit = i64::from(limit);
    let query = query!(
        &statement,
        post_id=post_id,
        current_user_id=current_user_id,
        max_repost_id=max_repost_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let reposts = rows.iter()
        .map(|row| {
            let id = row.try_get("id")?;
            let author = row.try_get("actor_profile")?;
            Ok((id, author))
        })
        .collect::<Result<_, DatabaseError>>()?;
    Ok(reposts)
}

/// Finds items reposted by user among given posts
pub(super) async fn find_reposted_by_user(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post.id
        FROM post
        WHERE post.id = ANY($2) AND EXISTS (
            SELECT 1 FROM post AS repost
            WHERE repost.author_id = $1 AND repost.repost_of_id = post.id
        )
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let reposted: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(reposted)
}

/// Returns Local reposts created before the specified date
pub async fn find_expired_reposts(
    db_client: &impl DatabaseClient,
    created_before: DateTime<Utc>,
) -> Result<Vec<Repost>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post
        FROM post
        WHERE
            repost_of_id IS NOT NULL
            AND object_id IS NULL
            AND created_at < $1
        ",
        &[&created_before],
    ).await?;
    let reposts = rows.iter()
        .map(Repost::try_from)
        .collect::<Result<_, _>>()?;
    Ok(reposts)
}

/// Finds all contexts (identified by top-level post)
/// updated before the specified date
/// that do not contain local posts, reposts, mentions, links or reactions.
pub async fn find_extraneous_posts(
    db_client: &impl DatabaseClient,
    updated_before: DateTime<Utc>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        WITH RECURSIVE context_post (context_id, post_id, created_at) AS (
            SELECT post.id, post.id, post.created_at
            FROM post
            WHERE
                -- top-level posts
                post.in_reply_to_id IS NULL
                AND post.repost_of_id IS NULL
                AND post.created_at < $1
            UNION
            SELECT context_post.context_id, post.id, post.created_at
            FROM post
            JOIN context_post ON (
                post.in_reply_to_id = context_post.post_id
                OR post.repost_of_id = context_post.post_id
            )
        )
        SELECT context.id
        FROM (
            SELECT
                context_post.context_id AS id,
                array_agg(context_post.post_id) AS posts,
                max(context_post.created_at) AS updated_at
            FROM context_post
            GROUP BY context_post.context_id
        ) AS context
        WHERE
            context.updated_at < $1
            -- no local replies or reposts in context
            AND NOT EXISTS (
                SELECT 1
                FROM post
                JOIN actor_profile ON post.author_id = actor_profile.id
                WHERE
                    post.id = ANY(context.posts)
                    AND (
                        actor_profile.user_id IS NOT NULL
                        OR actor_profile.portable_user_id IS NOT NULL
                    )
            )
            -- no local mentions in any post from context
            AND NOT EXISTS (
                SELECT 1
                FROM post_mention
                JOIN actor_profile ON post_mention.profile_id = actor_profile.id
                WHERE
                    post_mention.post_id = ANY(context.posts)
                    AND (
                        actor_profile.user_id IS NOT NULL
                        OR actor_profile.portable_user_id IS NOT NULL
                    )
            )
            -- no local reactions on any post from context
            AND NOT EXISTS (
                SELECT 1
                FROM post_reaction
                JOIN actor_profile ON post_reaction.author_id = actor_profile.id
                WHERE
                    post_reaction.post_id = ANY(context.posts)
                    AND (
                        actor_profile.user_id IS NOT NULL
                        OR actor_profile.portable_user_id IS NOT NULL
                    )
            )
            -- no local links to any post in context
            AND NOT EXISTS (
                SELECT 1
                FROM post_link
                JOIN post ON post_link.source_id = post.id
                JOIN actor_profile ON post.author_id = actor_profile.id
                WHERE
                    post_link.target_id = ANY(context.posts)
                    AND (
                        actor_profile.user_id IS NOT NULL
                        OR actor_profile.portable_user_id IS NOT NULL
                    )
            )
            -- no links to any post in context from other contexts
            AND NOT EXISTS (
                SELECT 1
                FROM post_link
                JOIN post ON post_link.source_id = post.id
                WHERE
                    post_link.target_id = ANY(context.posts)
                    AND post_link.source_id <> ALL(context.posts)
            )
            -- no bookmarks of any post in context
            AND NOT EXISTS (
                SELECT 1
                FROM bookmark
                WHERE bookmark.post_id = ANY(context.posts)
            )
            -- no votes to any poll in context
            AND NOT EXISTS (
                SELECT 1
                FROM poll_vote
                WHERE poll_vote.poll_id = ANY(context.posts)
            )
        ",
        &[&updated_before],
    ).await?;
    let ids: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Deletes post from database and returns collection of orphaned objects.
pub async fn delete_post(
    db_client: &mut impl DatabaseClient,
    post_id: Uuid,
) -> Result<DeletionQueue, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Select all posts that will be deleted.
    // This includes given post, its descendants and reposts.
    let posts_rows = transaction.query(
        "
        WITH RECURSIVE context (post_id) AS (
            SELECT post.id FROM post
            WHERE post.id = $1
            UNION
            SELECT post.id FROM post
            JOIN context ON (
                post.in_reply_to_id = context.post_id
                OR post.repost_of_id = context.post_id
            )
        )
        SELECT post_id FROM context
        ",
        &[&post_id],
    ).await?;
    let posts: Vec<Uuid> = posts_rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    // Get list of attached files
    let files_rows = transaction.query(
        "
        SELECT file_name
        FROM media_attachment WHERE post_id = ANY($1)
        ",
        &[&posts],
    ).await?;
    let files: Vec<String> = files_rows.iter()
        .map(|row| row.try_get("file_name"))
        .collect::<Result<_, _>>()?;
    // Get list of linked IPFS objects
    let ipfs_objects_rows = transaction.query(
        "
        SELECT ipfs_cid
        FROM media_attachment
        WHERE post_id = ANY($1) AND ipfs_cid IS NOT NULL
        UNION ALL
        SELECT ipfs_cid
        FROM post
        WHERE id = ANY($1) AND ipfs_cid IS NOT NULL
        ",
        &[&posts],
    ).await?;
    let ipfs_objects: Vec<String> = ipfs_objects_rows.iter()
        .map(|row| row.try_get("ipfs_cid"))
        .collect::<Result<_, _>>()?;
    // Update post counters
    transaction.execute(
        "
        UPDATE actor_profile
        SET post_count = post_count - post.count
        FROM (
            SELECT post.author_id, count(*) FROM post
            WHERE post.id = ANY($1)
            GROUP BY post.author_id
        ) AS post
        WHERE actor_profile.id = post.author_id
        ",
        &[&posts],
    ).await?;
    // Delete post
    let maybe_post_row = transaction.query_opt(
        "
        DELETE FROM post WHERE id = $1 AND repost_of_id IS NULL
        RETURNING post
        ",
        &[&post_id],
    ).await?;
    let post_row = maybe_post_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = post_row.try_get("post")?;
    // Update counters
    if let Some(parent_id) = db_post.in_reply_to_id {
        update_reply_count(&transaction, parent_id, -1).await?;
    };
    transaction.commit().await?;
    Ok(DeletionQueue { files, ipfs_objects })
}

pub async fn delete_repost(
    db_client: &mut impl DatabaseClient,
    repost_id: Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_post_row = transaction.query_opt(
        "
        DELETE FROM post WHERE id = $1 AND repost_of_id IS NOT NULL
        RETURNING post
        ",
        &[&repost_id],
    ).await?;
    let post_row = maybe_post_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = post_row.try_get("post")?;
    // Update counters
    let repost_of_id = db_post.repost_of_id.ok_or(DatabaseTypeError)?;
    update_repost_count(&transaction, repost_of_id, -1).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn search_posts(
    db_client: &impl DatabaseClient,
    text: &str,
    current_user_id: Uuid,
    limit: u16,
    offset: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {post_subqueries}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            -- can parse HTML documents
            to_tsvector('simple', post.content) @@ plainto_tsquery('simple', $1)
            AND repost_of_id IS NULL
            AND (
                -- posts published by the current user
                post.author_id = $2
                -- posts bookmarked by the current user
                OR EXISTS (
                    SELECT 1 FROM bookmark
                    WHERE
                        bookmark.post_id = post.id
                        AND bookmark.owner_id = $2
                )
                -- posts with reactions from the current user
                OR EXISTS (
                    SELECT 1 FROM post_reaction
                    WHERE
                        post_reaction.post_id = post.id
                        AND post_reaction.author_id = $2
                )
                -- posts where the current user is mentioned
                OR EXISTS (
                    SELECT 1 FROM post_mention
                    WHERE
                        post_mention.post_id = post.id
                        AND post_mention.profile_id = $2
                )
            )
        ORDER BY post.id DESC
        LIMIT $3 OFFSET $4
        ",
        post_subqueries=post_subqueries(),
    );
    let db_search_query = format!("%{}%", text);
    let rows = db_client.query(
        &statement,
        &[
            &db_search_query,
            &current_user_id,
            &i64::from(limit),
            &i64::from(offset),
        ],
    ).await?;
    let posts = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_post_count(
    db_client: &impl DatabaseClient,
    only_local: bool,
) -> Result<i64, DatabaseError> {
    let mut condition = format!(
        "
        post.in_reply_to_id IS NULL
        AND post.repost_of_id IS NULL
        AND post.visibility != {visibility_direct}
        ",
        visibility_direct=i16::from(Visibility::Direct),
    );
    if only_local {
        condition.push_str(" AND (
            actor_profile.user_id IS NOT NULL
            OR actor_profile.portable_user_id IS NOT NULL)");
    };
    let statement = format!(
        "
        SELECT count(post)
        FROM post
        JOIN actor_profile ON (post.author_id = actor_profile.id)
        WHERE {condition}
        ",
        condition=condition,
    );
    let row = db_client.query_one(&statement, &[]).await?;
    let count = row.try_get("count")?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use serial_test::serial;
    use crate::{
        custom_feeds::queries::{
            add_custom_feed_sources,
            create_custom_feed,
        },
        database::test_utils::create_test_database,
        posts::test_utils::{
            create_test_local_post,
            create_test_remote_post,
        },
        profiles::test_utils::{
            create_test_remote_profile,
        },
        relationships::queries::{
            follow,
            hide_reposts,
            subscribe,
            mute,
        },
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_post() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let mention_1 = create_test_user(db_client, "mention_1").await;
        let mention_2 = create_test_user(db_client, "mention_2").await;
        let post_data = PostCreateData {
            content: "test post".to_string(),
            mentions: vec![mention_2.id, mention_1.id],
            ..Default::default()
        };
        let post = create_post(db_client, author.id, post_data).await.unwrap();
        assert_eq!(post.content, "test post");
        assert_eq!(post.author.id, author.id);
        assert_eq!(post.attachments.is_empty(), true);
        assert_eq!(post.mentions[0].id, mention_2.id);
        assert_eq!(post.mentions[1].id, mention_1.id);
        assert_eq!(post.mentions.len(), 2);
        assert_eq!(post.tags.is_empty(), true);
        assert_eq!(post.links.is_empty(), true);
        assert_eq!(post.emojis.is_empty(), true);
        assert_eq!(post.reactions.is_empty(), true);
        assert_eq!(post.object_id, None);
        assert_eq!(post.updated_at, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_post_with_link() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data_1 = PostCreateData::default();
        let post_1 = create_post(db_client, author.id, post_data_1).await.unwrap();
        let post_data_2 = PostCreateData {
            links: vec![post_1.id],
            ..Default::default()
        };
        let post_2 = create_post(db_client, author.id, post_data_2).await.unwrap();
        assert_eq!(post_2.links, vec![post_1.id]);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_repost() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, author.id, post_data).await.unwrap();
        let repost_data = PostCreateData::repost(
            post.id,
            Visibility::Public,
            None,
        );
        let repost = create_post(
            db_client,
            author.id,
            repost_data,
        ).await.unwrap();
        assert_eq!(repost.content, "");
        assert_eq!(repost.author.id, author.id);
        assert_eq!(repost.repost_of_id, Some(post.id));
        assert_eq!(repost.object_id, None);

        let repost_details = get_repost_by_author(
            db_client,
            post.id,
            author.id,
        ).await.unwrap();
        assert_eq!(repost_details.id, repost.id);
        assert_eq!(repost_details.has_deprecated_ap_id, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_post() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, author.id, post_data).await.unwrap();
        let post_data = PostUpdateData {
            content: "test update".to_string(),
            updated_at: Some(Utc::now()),
            ..Default::default()
        };
        let (post, deletion_queue) =
            update_post(db_client, post.id, post_data).await.unwrap();
        assert_eq!(post.content, "test update");
        assert_eq!(post.updated_at.is_some(), true);
        assert_eq!(deletion_queue.files.len(), 0);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_post() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, author.id, post_data).await.unwrap();
        let deletion_queue = delete_post(db_client, post.id).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 0);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_repost() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post = create_test_local_post(db_client, author.id, "test").await;
        let repost_data = PostCreateData::repost(
            post.id,
            Visibility::Public,
            None,
        );
        let repost = create_post(
            db_client,
            author.id,
            repost_data,
        ).await.unwrap();
        delete_repost(db_client, repost.id).await.unwrap();

        let post = get_post_by_id(db_client, post.id).await.unwrap();
        assert_eq!(post.repost_count, 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_home_timeline() {
        let db_client = &mut create_test_database().await;
        let current_user = create_test_user(db_client, "test").await;
        // Current user's post
        let post_data_1 = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post_1 = create_post(db_client, current_user.id, post_data_1).await.unwrap();
        // Current user's direct message
        let post_data_2 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let post_2 = create_post(db_client, current_user.id, post_data_2).await.unwrap();
        // Another user's public post
        let user_1 = create_test_user(db_client, "another-user").await;
        let post_data_3 = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post_3 = create_post(db_client, user_1.id, post_data_3).await.unwrap();
        // Direct message from another user to current user
        let post_data_4 = PostCreateData {
            content: "test post".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![current_user.id],
            ..Default::default()
        };
        let post_4 = create_post(db_client, user_1.id, post_data_4).await.unwrap();
        // Followers-only post from another user
        let post_data_5 = PostCreateData {
            content: "followers only".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_5 = create_post(db_client, user_1.id, post_data_5).await.unwrap();
        // Followed user's public post
        let user_2 = create_test_user(db_client, "followed").await;
        follow(db_client, current_user.id, user_2.id).await.unwrap();
        let post_data_6 = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post_6 = create_post(db_client, user_2.id, post_data_6).await.unwrap();
        // Followed user's repost
        let post_data_7 = PostCreateData::repost(
            post_3.id,
            Visibility::Public,
            None,
        );
        let post_7 = create_post(db_client, user_2.id, post_data_7).await.unwrap();
        // Direct message from followed user sent to another user
        let post_data_8 = PostCreateData {
            content: "test post".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![user_1.id],
            ..Default::default()
        };
        let post_8 = create_post(db_client, user_2.id, post_data_8).await.unwrap();
        // Followers-only post from followed user
        let post_data_9 = PostCreateData {
            content: "followers only".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_9 = create_post(db_client, user_2.id, post_data_9).await.unwrap();
        // Subscribers-only post by followed user
        let post_data_10 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_10 = create_post(db_client, user_2.id, post_data_10).await.unwrap();
        // Subscribers-only post by subscription (without mention)
        let user_3 = create_test_user(db_client, "subscription").await;
        subscribe(db_client, current_user.id, user_3.id).await.unwrap();
        let post_data_11 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_11 = create_post(db_client, user_3.id, post_data_11).await.unwrap();
        // Subscribers-only post by subscription (with mention)
        let post_data_12 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            mentions: vec![current_user.id],
            ..Default::default()
        };
        let post_12 = create_post(db_client, user_3.id, post_data_12).await.unwrap();
        // Repost from followed user if hiding reposts
        let user_4 = create_test_user(db_client, "hide reposts").await;
        follow(db_client, current_user.id, user_4.id).await.unwrap();
        hide_reposts(db_client, current_user.id, user_4.id).await.unwrap();
        let post_data_13 = PostCreateData::repost(
            post_3.id,
            Visibility::Public,
            None,
        );
        let post_13 = create_post(db_client, user_4.id, post_data_13).await.unwrap();
        // Post from followed user if muted
        let user_5 = create_test_user(db_client, "muted").await;
        follow(db_client, current_user.id, user_5.id).await.unwrap();
        mute(db_client, current_user.id, user_5.id).await.unwrap();
        let post_data_14 = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post_14 = create_post(db_client, user_5.id, post_data_14).await.unwrap();

        let timeline = get_home_timeline(db_client, current_user.id, None, 20).await.unwrap();
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_3.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_4.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_5.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_6.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_7.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_8.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_9.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_10.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_11.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_12.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_13.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_14.id), false);
        assert_eq!(timeline.len(), 8);
    }

    #[tokio::test]
    #[serial]
    async fn test_public_timeline() {
        let db_client = &mut create_test_database().await;
        let current_user = create_test_user(db_client, "test").await;
        let remote_profile = create_test_remote_profile(
            db_client,
            "test",
            "example.com",
            "https://example.com/users/1",
        ).await;
        let post_data_1 = PostCreateData {
            content: "test post".to_string(),
            object_id: Some("https://example.com/objects/1".to_string()),
            ..Default::default()
        };
        let post_1 = create_post(db_client, remote_profile.id, post_data_1)
            .await.unwrap();
        let post_data_2 = PostCreateData {
            content: "test post".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![current_user.id],
            object_id: Some("https://example.com/objects/2".to_string()),
            ..Default::default()
        };
        let post_2 = create_post(db_client, remote_profile.id, post_data_2)
            .await.unwrap();

        // As local user
        let timeline = get_public_timeline(
            db_client,
            Some(current_user.id),
            false,
            None,
            20,
        ).await.unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), false);

        // As guest
        let timeline = get_public_timeline(
            db_client,
            None,
            false,
            None,
            20,
        ).await.unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), false);
    }

    #[tokio::test]
    #[serial]
    async fn test_direct_timeline() {
        let db_client = &mut create_test_database().await;
        let current_user = create_test_user(db_client, "test").await;
        let user_1 = create_test_user(db_client, "user1").await;
        let user_2 = create_test_user(db_client, "user2").await;
        // Incoming DM
        let post_data_1 = PostCreateData {
            content: "test dm".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![current_user.id],
            ..Default::default()
        };
        let post_1 = create_post(db_client, user_1.id, post_data_1)
            .await.unwrap();
        // Public post with mention
        let post_data_2 = PostCreateData {
            content: "test public".to_string(),
            visibility: Visibility::Public,
            mentions: vec![current_user.id],
            ..Default::default()
        };
        let post_2 = create_post(db_client, user_1.id, post_data_2)
            .await.unwrap();
        // Message to other user
        let post_data_3 = PostCreateData {
            content: "test dm 2".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![user_1.id],
            ..Default::default()
        };
        let post_3 = create_post(db_client, user_2.id, post_data_3)
            .await.unwrap();

        let timeline = get_direct_timeline(
            db_client,
            current_user.id,
            None,
            20,
        ).await.unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_3.id), false);
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_timeline_guest() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "test").await;
        let another_user = create_test_remote_profile(
            db_client,
            "test",
            "social.example",
            "https://social.example/users/1",
        ).await;
        // Public post
        let post_data_1 = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post_1 = create_post(db_client, user.id, post_data_1).await.unwrap();
        // Followers only post
        let post_data_2 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_2 = create_post(db_client, user.id, post_data_2).await.unwrap();
        // Subscribers only post
        let post_data_3 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_3 = create_post(db_client, user.id, post_data_3).await.unwrap();
        // Direct message
        let post_data_4 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let post_4 = create_post(db_client, user.id, post_data_4).await.unwrap();
        // Reply
        let reply_data = PostCreateData {
            context: PostContext::reply_to(&post_1),
            content: "my reply".to_string(),
            ..Default::default()
        };
        let reply = create_post(db_client, user.id, reply_data).await.unwrap();
        // Repost
        let another_user_post_1 = create_test_remote_post(
            db_client,
            another_user.id,
            "public post 1",
            "https://social.example/posts/1",
        ).await;
        let repost_data_1 = PostCreateData::repost(
            another_user_post_1.id,
            Visibility::Public,
            None,
        );
        let repost_1 = create_post(db_client, user.id, repost_data_1).await.unwrap();
        // Followers only repost
        let another_user_post_2 = create_test_remote_post(
            db_client,
            another_user.id,
            "public post 2",
            "https://social.example/posts/2",
        ).await;
        let repost_data_2 = PostCreateData::repost(
            another_user_post_2.id,
            Visibility::Followers,
            None,
        );
        let repost_2 = create_post(db_client, user.id, repost_data_2).await.unwrap();

        // Anonymous viewer
        let timeline = get_posts_by_author(
            db_client,
            user.id,
            None,
            false,
            true,
            false,
            false,
            None,
            10,
        ).await.unwrap();
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_3.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_4.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == reply.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == repost_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == repost_2.id), false);
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_timeline_private_repost() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test1").await;
        let post_data = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, user_1.id, post_data).await.unwrap();
        let user_2 = create_test_user(db_client, "test2").await;
        let repost_data = PostCreateData::repost(
            post.id,
            Visibility::Followers,
            None,
        );
        let repost = create_post(db_client, user_2.id, repost_data).await.unwrap();

        let timeline = get_posts_by_author(
            db_client,
            user_2.id,
            Some(user_1.id),
            false, // don't include replies
            true, // include reposts
            false, // not only pinned
            false, // not only media
            None,
            10,
        ).await.unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.iter().any(|item| item.id == repost.id), true);
    }

    #[tokio::test]
    #[serial]
    async fn test_custom_feed_timeline() {
        let db_client = &mut create_test_database().await;
        let viewer = create_test_user(db_client, "viewer").await;
        let author_1 = create_test_user(db_client, "author_1").await;
        let author_2 = create_test_user(db_client, "author_2").await;
        let feed = create_custom_feed(
            db_client,
            viewer.id,
            "test",
        ).await.unwrap();
        add_custom_feed_sources(
            db_client,
            feed.id,
            &[author_1.id],
        ).await.unwrap();
        let post_1 = create_test_local_post(
            db_client,
            author_1.id,
            "test post 1",
        ).await;
        let post_2 = create_test_local_post(
            db_client,
            author_2.id,
            "test post 2",
        ).await;
        let timeline = get_custom_feed_timeline(
            db_client,
            feed.id,
            viewer.id,
            None,
            10,
        ).await.unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), false);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_thread() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test_1").await;
        let user_2 = create_test_user(db_client, "test_2").await;
        let post_data_1 = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post_1 = create_post(db_client, user_1.id, post_data_1).await.unwrap();
        let post_data_2 = PostCreateData {
            context: PostContext::reply_to(&post_1),
            content: "reply".to_string(),
            ..Default::default()
        };
        let post_2 = create_post(db_client, user_2.id, post_data_2).await.unwrap();
        let post_data_3 = PostCreateData {
            context: PostContext::reply_to(&post_1),
            content: "direct reply".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![user_1.id],
            ..Default::default()
        };
        let post_3 = create_post(db_client, user_2.id, post_data_3).await.unwrap();
        let post_data_4 = PostCreateData {
            context: PostContext::reply_to(&post_1),
            content: "hidden reply".to_string(),
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let post_4 = create_post(db_client, user_2.id, post_data_4).await.unwrap();

        let thread = get_thread(
            db_client,
            post_2.id,
            Some(user_1.id),
        ).await.unwrap();
        assert_eq!(thread.len(), 3);
        assert_eq!(thread[0].id, post_1.id);
        assert_eq!(thread[1].id, post_2.id);
        assert_eq!(thread[2].id, post_3.id);

        let error = get_thread(
            db_client,
            post_4.id,
            Some(user_1.id),
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "post not found");

        let (root, thread) = get_conversation_items(
            db_client,
            post_1.expect_conversation().id,
            Some(user_1.id),
        ).await.unwrap();
        assert_eq!(root.id, post_1.id);
        assert_eq!(thread.len(), 3);

        let (root, thread) = get_conversation_items(
            db_client,
            post_1.expect_conversation().id,
            None,
        ).await.unwrap();
        assert_eq!(root.id, post_1.id);
        assert_eq!(thread.len(), 2);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_thread_followers_only_conversation() {
        let db_client = &mut create_test_database().await;
        let user_1 = create_test_user(db_client, "test_1").await;
        let user_2 = create_test_user(db_client, "test_2").await;
        follow(db_client, user_2.id, user_1.id).await.unwrap();
        let user_3 = create_test_user(db_client, "test_3").await;
        follow(db_client, user_3.id, user_2.id).await.unwrap();
        let post_data_1 = PostCreateData {
            context: PostContext::Top { audience: None },
            content: "my post".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_1 = create_post(db_client, user_1.id, post_data_1).await.unwrap();
        let post_data_2 = PostCreateData {
            context: PostContext::reply_to(&post_1),
            content: "reply".to_string(),
            visibility: Visibility::Conversation,
            ..Default::default()
        };
        let post_2 = create_post(db_client, user_2.id, post_data_2).await.unwrap();

        let thread = get_thread(
            db_client,
            post_2.id,
            Some(user_1.id),
        ).await.unwrap();
        assert_eq!(thread.len(), 2);

        let thread = get_thread(
            db_client,
            post_2.id,
            Some(user_2.id),
        ).await.unwrap();
        assert_eq!(thread.len(), 2);

        let error = get_thread(
            db_client,
            post_2.id,
            Some(user_3.id),
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "post not found");

        let error = get_thread(
            db_client,
            post_1.id,
            None,
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "post not found");

        let (root, thread) = get_conversation_items(
            db_client,
            post_1.expect_conversation().id,
            Some(user_1.id),
        ).await.unwrap();
        assert_eq!(root.id, post_1.id);
        assert_eq!(thread.len(), 2);

        let error = get_conversation_items(
            db_client,
            post_1.expect_conversation().id,
            None,
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "conversation not found");
    }

    #[tokio::test]
    #[serial]
    async fn test_get_post_reposts() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "test").await;
        let remote_user_1 = create_test_remote_profile(
            db_client,
            "test1",
            "social.example",
            "https://social.example/users/1",
        ).await;
        let remote_user_2 = create_test_remote_profile(
            db_client,
            "test2",
            "social.example",
            "https://social.example/users/2",
        ).await;
        let post = create_test_local_post(
            db_client,
            user.id,
            "test post",
        ).await;
        // Public repost
        let repost_data_1 = PostCreateData::repost(
            post.id,
            Visibility::Public,
            Some("https://social.example/activity1".to_owned()),
        );
        let repost_1 = create_post(
            db_client,
            remote_user_1.id,
            repost_data_1,
        ).await.unwrap();
        // Followers only repost
        let repost_data_2 = PostCreateData::repost(
            post.id,
            Visibility::Followers,
            Some("https://social.example/activity2".to_owned()),
        );
        let repost_2 = create_post(
            db_client,
            remote_user_2.id,
            repost_data_2,
        ).await.unwrap();

        let reposts = get_post_reposts(
            db_client,
            post.id,
            None,
            None,
            10,
        ).await.unwrap();
        assert_eq!(reposts.len(), 1);
        assert_eq!(reposts.iter().any(|(repost_id, _)| *repost_id == repost_1.id), true);
        assert_eq!(reposts.iter().any(|(repost_id, _)| *repost_id == repost_2.id), false);
        let reposts = get_post_reposts(
            db_client,
            post.id,
            Some(user.id),
            None,
            10,
        ).await.unwrap();
        assert_eq!(reposts.len(), 2);
    }

    #[tokio::test]
    #[serial]
    async fn test_find_expired_reposts() {
        let db_client = &mut create_test_database().await;
        let created_before = Utc::now() - Duration::days(1);
        let reposts = find_expired_reposts(
            db_client,
            created_before,
        ).await.unwrap();
        assert_eq!(reposts.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_find_extraneous_posts() {
        let db_client = &mut create_test_database().await;
        let author = create_test_remote_profile(
            db_client,
            "test",
            "social.example",
            "https://social.example/users/1",
        ).await;
        let post_data_1 = PostCreateData {
            content: "test post".to_string(),
            object_id: Some("https://social.example/objects/1".to_string()),
            created_at: Utc::now(),
            ..Default::default()
        };
        let _post_1 = create_post(
            db_client,
            author.id,
            post_data_1,
        ).await.unwrap();
        let post_data_2 = PostCreateData {
            content: "test post".to_string(),
            object_id: Some("https://social.example/objects/2".to_string()),
            created_at: Utc::now() - Duration::days(7),
            ..Default::default()
        };
        let post_2 = create_post(
            db_client,
            author.id,
            post_data_2,
        ).await.unwrap();

        let updated_before = Utc::now() - Duration::days(1);
        let result = find_extraneous_posts(
            db_client,
            updated_before,
        ).await.unwrap();
        assert_eq!(result, vec![post_2.id]);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_posts() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "viewer").await;
        let author = create_test_user(db_client, "author").await;
        let post_1 = create_test_local_post(
            db_client,
            user.id,
            "test post 1",
        ).await;
        let _post_2 = create_test_local_post(
            db_client,
            author.id,
            "test post 2",
        ).await;
        let results = search_posts(
            db_client,
            "post",
            user.id,
            5,
            0, // no offset
        ).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, post_1.id);
    }
}
