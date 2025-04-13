use std::path::PathBuf;

use anyhow::{anyhow, Error};
use chrono::{DateTime, Utc};
use clap::Parser;

use apx_core::media_type::sniff_media_type;
use mitra_config::Config;
use mitra_models::{
    attachments::queries::create_attachment,
    database::DatabaseClient,
    media::types::MediaInfo,
    posts::{
        queries::create_post,
        types::{PostContext, PostCreateData, Visibility},
    },
    profiles::helpers::get_profile_by_id_or_acct,
};
use mitra_services::media::MediaStorage;
use mitra_utils::{
    files::FileSize,
    id::datetime_to_ulid,
};
use mitra_validators::{
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
    /// Date
    created_at: DateTime<Utc>,
    /// Media attachment(s)
    #[arg(long)]
    attachment: Vec<PathBuf>,
}

impl CreatePost {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let author = get_profile_by_id_or_acct(db_client, &self.author).await?;
        if !author.is_local() {
            return Err(anyhow!("author must be local"));
        };
        let content = clean_remote_content(&self.content);
        let mut attachments = vec![];
        let storage = MediaStorage::new(config);
        for attachment_path in self.attachment.iter() {
            let file_data = std::fs::read(attachment_path)?;
            let media_type = sniff_media_type(&file_data)
                .ok_or(anyhow!("unknown media type"))?;
            if !config.limits.media.supported_media_types().contains(&media_type.as_str()) {
                return Err(anyhow!("media type {media_type} is not supported"));
            };
            if file_data.len() > config.limits.media.file_size_limit {
                let limit = FileSize::new(config.limits.media.file_size_limit);
                return Err(anyhow!("file size must be less than {limit}"));
            };
            let file_info = storage.save_file(file_data, &media_type)?;
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
        let post = create_post(db_client, author.id, post_data).await?;
        println!("post created: {}", post.id);
        Ok(())
    }
}
