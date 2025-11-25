use anyhow::{anyhow, Error};
use apx_core::{
    media_type::sniff_media_type,
    url::http_uri::HttpUri,
};
use apx_sdk::fetch::fetch_media;
use clap::Parser;

use mitra_activitypub::agent::build_federation_agent;
use mitra_adapters::media::delete_orphaned_media;
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    emojis::{
        helpers::get_emoji_by_name,
        queries::{
            create_or_update_local_emoji,
            delete_emoji,
            get_emoji_by_name_and_hostname,
        },
    },
    media::types::{MediaInfo, PartialMediaInfo},
    profiles::types::Origin::Local,
};
use mitra_services::media::MediaStorage;
use mitra_utils::files::FileSize;
use mitra_validators::{
    emojis::{
        clean_emoji_name,
        validate_emoji_name,
        EMOJI_LOCAL_MEDIA_TYPES,
        EMOJI_REMOTE_MEDIA_TYPES,
    },
};

fn validate_local_emoji_data(
    config: &Config,
    emoji_name: &str,
    file_data: &[u8],
    media_type: &str,
) -> Result<(), Error> {
    if validate_emoji_name(emoji_name, Local).is_err() {
        return Err(anyhow!("invalid emoji name"));
    };
    if !EMOJI_LOCAL_MEDIA_TYPES.contains(&media_type) {
        return Err(anyhow!("media type {media_type} is not supported"));
    };
    if file_data.len() > config.limits.media.emoji_local_size_limit {
        return Err(anyhow!(
            "emoji file size must be less than {}",
            FileSize::new(config.limits.media.emoji_local_size_limit),
        ));
    };
    Ok(())
}

/// Add custom emoji to local collection
#[derive(Parser)]
pub struct AddEmoji {
    emoji_name: String,
    /// File path or URL
    location: String,
}

impl AddEmoji {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let (file_data, media_type) = if
            HttpUri::parse(&self.location).is_ok()
        {
            let agent = build_federation_agent(&config.instance(), None);
            fetch_media(
                &agent,
                &self.location,
                &EMOJI_REMOTE_MEDIA_TYPES, // media type will be checked later
                config.limits.media.file_size_limit, // size will be checked later
            ).await?
        } else {
            let file_data = std::fs::read(&self.location)?;
            let media_type = sniff_media_type(&file_data)
                .ok_or(anyhow!("unknown media type"))?;
            (file_data, media_type)
        };
        validate_local_emoji_data(config, &self.emoji_name, &file_data, &media_type)?;
        let media_storage = MediaStorage::new(config);
        let file_info = media_storage.save_file(file_data, &media_type)?;
        let image = MediaInfo::local(file_info);
        let (_, deletion_queue) = create_or_update_local_emoji(
            db_client,
            &self.emoji_name,
            image,
        ).await?;
        deletion_queue.into_job(db_client).await?;
        println!("added emoji to local collection");
        Ok(())
    }
}

/// Copy cached custom emoji to local collection
#[derive(Parser)]
#[command(visible_alias = "steal-emoji")]
pub struct ImportEmoji {
    emoji_name: String,
    hostname: String,
}

impl ImportEmoji {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let emoji_name = clean_emoji_name(&self.emoji_name);
        let emoji = get_emoji_by_name_and_hostname(
            db_client,
            emoji_name,
            &self.hostname,
        ).await?;
        let media_storage = MediaStorage::new(config);
        let (file_data, media_type) = match emoji.image {
            PartialMediaInfo::File { file_info, .. } => {
                let file_data = media_storage.read_file(&file_info.file_name)?;
                let media_type = sniff_media_type(&file_data)
                    .ok_or(anyhow!("unknown media type"))?;
                (file_data, media_type)
            },
            PartialMediaInfo::Link { url, .. } => {
                let agent = build_federation_agent(&config.instance(), None);
                fetch_media(
                    &agent,
                    &url,
                    &EMOJI_REMOTE_MEDIA_TYPES, // media type will be checked later
                    config.limits.media.file_size_limit, // size will be checked later
                ).await?
            },
        };
        validate_local_emoji_data(config, &emoji.emoji_name, &file_data, &media_type)?;
        let file_info = media_storage.save_file(file_data, &media_type)?;
        let image = MediaInfo::local(file_info);
        let (_, deletion_queue) = create_or_update_local_emoji(
            db_client,
            &emoji.emoji_name,
            image,
        ).await?;
        deletion_queue.into_job(db_client).await?;
        println!("added emoji to local collection");
        Ok(())
    }
}

/// Delete custom emoji
#[derive(Parser)]
pub struct DeleteEmoji {
    emoji_name: String,
    hostname: Option<String>,
}

impl DeleteEmoji {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let emoji = get_emoji_by_name(
            db_client,
            &self.emoji_name,
            self.hostname.as_deref(),
        ).await?;
        let deletion_queue = delete_emoji(db_client, emoji.id).await?;
        delete_orphaned_media(config, db_client, deletion_queue).await?;
        println!("emoji deleted");
        Ok(())
    }
}
