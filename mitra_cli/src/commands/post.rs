use std::path::PathBuf;

use anyhow::{anyhow, Error};
use apx_sdk::{
    core::{
        http_url::HttpUrl,
        media_type::sniff_media_type,
    },
    deserialization::parse_into_id_array,
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
};
use mitra_adapters::{
    posts::check_post_limits,
};
use mitra_config::Config;
use mitra_models::{
    attachments::queries::create_attachment,
    database::{get_database_client, DatabaseConnectionPool},
    media::types::MediaInfo,
    posts::{
        queries::{create_post, delete_post, get_post_by_id},
        types::{PostContext, PostCreateData, Visibility},
    },
    users::helpers::get_user_by_id_or_name,
};
use mitra_services::media::MediaStorage;
use mitra_utils::{
    files::FileSize,
    id::datetime_to_ulid,
};
use mitra_validators::{
    common::Origin::Local,
    posts::{
        clean_remote_content,
        validate_post_create_data,
    },
};

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
}

impl CreatePost {
    pub async fn execute(
        &self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let author = {
            let db_client = &**get_database_client(db_pool).await?;
            get_user_by_id_or_name(db_client, &self.author).await?
        };
        let content = clean_remote_content(&self.content);
        let mut attachments = vec![];
        let storage = MediaStorage::new(config);
        for location in self.attachment.iter() {
            let (file_data, media_type) = if HttpUrl::parse(location).is_ok() {
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
            id: Some(datetime_to_ulid(self.created_at)),
            context: PostContext::Top { audience: None },
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
        &self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let outbox_data = std::fs::read_to_string(&self.outbox_path)?;
        let outbox: JsonValue = serde_json::from_str(&outbox_data)?;
        let activities = outbox["orderedItems"].as_array()
            .ok_or(anyhow!("'orderedItems' not found"))?;
        for activity in activities {
            // Only public top-level posts
            if activity["type"].as_str() != Some("Create") {
                continue;
            };
            let object = &activity["object"];
            if !object["inReplyTo"].is_null() {
                continue;
            };
            let Ok(audience) = parse_into_id_array(&object["to"]) else {
                continue;
            };
            if !audience.iter().any(is_public) {
                continue;
            };
            let content = object["content"].as_str()
                .ok_or(anyhow!("'content' not found"))?;
            let published = object["published"].as_str()
                .ok_or(anyhow!("'published' not found"))?;
            let created_at = DateTime::parse_from_rfc3339(published)?;
            let command = CreatePost {
                author: self.author.clone(),
                content: content.to_string(),
                created_at: created_at.into(),
                attachment: vec![],
            };
            command.execute(config, db_pool).await?;
        };
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
        &self,
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
