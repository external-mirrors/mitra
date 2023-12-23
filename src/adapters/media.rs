use mitra_config::Config;
use mitra_models::cleanup::DeletionQueue;
use mitra_services::{
    ipfs::{store as ipfs_store},
    media::MediaStorage,
};

pub fn delete_files(
    storage: &MediaStorage,
    files: Vec<String>,
) -> () {
    for file_name in files {
        match storage.delete_file(&file_name) {
            Ok(_) => log::info!("removed file {}", file_name),
            Err(err) => {
                log::warn!("failed to remove file {} ({})", file_name, err);
            },
        };
    };
}

pub async fn delete_media(
    config: &Config,
    queue: DeletionQueue,
) -> () {
    if !queue.files.is_empty() {
        let storage = MediaStorage::from(config);
        delete_files(&storage, queue.files);
    };
    if !queue.ipfs_objects.is_empty() {
        match &config.ipfs_api_url {
            Some(ipfs_api_url) => {
                ipfs_store::remove(ipfs_api_url, queue.ipfs_objects).await
                    .unwrap_or_else(|err| log::error!("{}", err));
            },
            None => {
                log::error!(
                    "can not remove objects because IPFS API URL is not set: {:?}",
                    queue.ipfs_objects,
                );
            },
        }
    }
}
