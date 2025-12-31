use std::path::PathBuf;

use anyhow::{anyhow, Error};
use apx_sdk::{
    core::{
        media_type::sniff_media_type,
        url::http_uri::HttpUri,
    },
    fetch::fetch_media,
    utils::is_public,
};
use chrono::{DateTime, Utc};
use clap::Parser;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_activitypub::{
    adapters::posts::delete_local_post,
    agent::build_federation_agent,
    builders::{
        create_note::build_create_note,
        collection::OrderedCollection,
    },
    handlers::note::{Attachment, AttributedObject},
    identifiers::{local_actor_id, LocalActorCollection},
};
use mitra_adapters::{
    posts::check_post_limits,
};
use mitra_config::Config;
use mitra_models::{
    attachments::queries::create_attachment,
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    media::types::MediaInfo,
    posts::{
        helpers::add_related_posts,
        queries::{
            create_post,
            delete_post,
            get_post_by_id,
            get_posts_by_author,
        },
        types::{PostContext, PostCreateData, Visibility},
    },
    profiles::types::Origin::Local,
    users::helpers::get_user_by_id_or_name,
};
use mitra_services::media::{MediaServer, MediaStorage};
use mitra_utils::{
    files::FileSize,
    id::generate_deterministic_ulid,
};
use mitra_validators::{
    posts::{
        clean_remote_content,
        validate_post_create_data,
    },
};

fn generate_post_id(
    author_id: Uuid,
    content: &str,
    created_at: DateTime<Utc>,
) -> Uuid {
    generate_deterministic_ulid(
        &format!("{}{}", author_id, content),
        created_at,
    )
}

/// Create a post with the specified timestamp
#[derive(Parser)]
pub struct CreatePost {
    /// Author (username or ID)
    author: String,
    /// HTML content
    content: String,
    /// Date (YYYY-MM-DDThh:mm:ss±hh:mm)
    created_at: DateTime<Utc>,
    /// Media attachment file path or URL (this option can be used more than once)
    #[arg(long)]
    attachment: Vec<String>,
    /// Unique post ID
    #[arg(long)]
    id: Option<Uuid>,
}

impl CreatePost {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let author = get_user_by_id_or_name(
            db_client_await!(db_pool),
            &self.author,
        ).await?;
        let post_id = self.id.unwrap_or_else(|| {
            generate_post_id(author.id, &self.content, self.created_at)
        });
        let content = clean_remote_content(&self.content);
        let mut attachments = vec![];
        let storage = MediaStorage::new(config);
        for location in self.attachment.iter() {
            let (file_data, media_type) = if HttpUri::parse(location).is_ok() {
                let agent = build_federation_agent(&config.instance(), None);
                fetch_media(
                    &agent,
                    location,
                    &config.limits.media.supported_media_types(),
                    config.limits.media.file_size_limit,
                ).await?
            } else {
                let file_data = std::fs::read(location)?;
                let media_type = sniff_media_type(&file_data)
                    .ok_or(anyhow!("unknown media type"))?;
                (file_data, media_type)
            };
            if !config.limits.media.supported_media_types().contains(&media_type.as_str()) {
                return Err(anyhow!("media type {media_type} is not supported"));
            };
            if file_data.len() > config.limits.media.file_size_limit {
                let limit = FileSize::new(config.limits.media.file_size_limit);
                return Err(anyhow!("file size must be less than {limit}"));
            };
            let file_info = storage.save_file(file_data, &media_type)?;
            let db_client = &**get_database_client(db_pool).await?;
            let attachment = create_attachment(
                db_client,
                author.id,
                MediaInfo::local(file_info),
                None,
            ).await?;
            attachments.push(attachment.id);
        };
        let post_data = PostCreateData {
            id: Some(post_id),
            context: PostContext::new_public(),
            content: content,
            content_source: None,
            language: None,
            visibility: Visibility::Public,
            is_sensitive: false,
            poll: None,
            attachments: attachments,
            mentions: vec![],
            tags: vec![],
            links: vec![],
            emojis: vec![],
            url: None,
            object_id: None,
            created_at: self.created_at,
        };
        validate_post_create_data(&post_data)?;
        check_post_limits(&config.limits.posts, &post_data.attachments, Local)?;
        let db_client = &mut **get_database_client(db_pool).await?;
        let post = create_post(db_client, author.id, post_data).await?;
        println!("post created: {}", post.id);
        Ok(())
    }
}

/// Import posts from outbox
#[derive(Parser)]
pub struct ImportPosts {
    /// Author (username or ID)
    author: String,
    /// Path to outbox.json
    outbox_path: PathBuf,
}

impl ImportPosts {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let author = get_user_by_id_or_name(
            db_client_await!(db_pool),
            &self.author,
        ).await?;
        let outbox_data = std::fs::read_to_string(&self.outbox_path)?;
        let outbox: JsonValue = serde_json::from_str(&outbox_data)?;
        let activities = outbox["orderedItems"].as_array()
            .ok_or(anyhow!("'orderedItems' not found"))?;
        for activity in activities {
            // Only public top-level posts
            if activity["type"].as_str() != Some("Create") {
                continue;
            };
            let object: AttributedObject =
                serde_json::from_value(activity["object"].clone())?;
            if object.in_reply_to.is_some() {
                continue;
            };
            if !object.audience().iter().any(is_public) {
                continue;
            };
            let content = object.content
                .ok_or(anyhow!("'content' not found"))?;
            let created_at = object.published
                .ok_or(anyhow!("'published' not found"))?;
            let attachments = object.attachment.iter()
                .filter_map(|attachment| match attachment {
                    Attachment::Media(media) => Some(media.url.clone()),
                    _ => None,
                })
                .map(|location| {
                    if location.starts_with("/media_attachments") {
                        // Mastodon archive
                        location.trim_start_matches('/').to_owned()
                    } else {
                        location
                    }
                })
                .collect();
            let post_id = generate_post_id(author.id, &content, created_at);
            let command = CreatePost {
                author: self.author.clone(),
                content: content,
                created_at: created_at,
                attachment: attachments,
                id: Some(post_id),
            };
            command.execute(config, db_pool)
                .await
                .or_else(|error| {
                    if let Some(DatabaseError::AlreadyExists(_)) = error.downcast_ref() {
                        println!("post already exists: {post_id}");
                        Ok(())
                    } else {
                        Err(error)
                    }
                })?;
        };
        Ok(())
    }
}

/// Export posts as outbox JSON
#[derive(Parser)]
pub struct ExportPosts {
    /// Author (username or ID)
    author: String,
}

impl ExportPosts {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let instance = config.instance();
        let author = get_user_by_id_or_name(
            db_client,
            &self.author,
        ).await?;
        let mut posts = get_posts_by_author(
            db_client,
            author.id,
            None, // include only public posts
            true, // include replies
            false, // don't include reposts
            false, // not only pinned
            false, // not only media
            None, // no max ID
            OrderedCollection::PAGE_SIZE,
        ).await?;
        add_related_posts(db_client, posts.iter_mut().collect()).await?;
        let media_server = MediaServer::new(config);
        let activities = posts.iter().map(|post| {
            let activity = build_create_note(
                instance.uri(),
                &media_server,
                post,
            );
            serde_json::to_value(activity)
                .expect("activity should be serializable")
        }).collect();
        let actor_id = local_actor_id(
            instance.uri_str(),
            &author.profile.username,
        );
        let collection_id = LocalActorCollection::Outbox.of(&actor_id);
        let collection = OrderedCollection::new_with_items(
            collection_id,
            activities,
        );
        let collection_json = serde_json::to_value(collection)
            .expect("collection should be serializable");
        println!("{}", collection_json);
        Ok(())
    }
}

/// Delete post
#[derive(Parser)]
pub struct DeletePost {
    id: Uuid,
}

impl DeletePost {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let post = get_post_by_id(db_client, self.id).await?;
        if post.author.is_local() {
            delete_local_post(
                config,
                db_client,
                &post,
            ).await?;
        } else {
            let deletion_queue = delete_post(db_client, post.id).await?;
            deletion_queue.into_job(db_client).await?;
        };
        println!("post deleted");
        Ok(())
    }
}
