use apx_core::{
    crypto::eddsa::{
        create_eddsa_signature,
        Ed25519SecretKey,
    },
    url::common::url_encode,
};

use mitra_config::Config;
use mitra_models::media::types::PartialMediaInfo;
use mitra_services::media::MediaServer;

pub struct ClientMediaServer {
    media_server: MediaServer,
    media_proxy_key: Ed25519SecretKey,
    base_url: String,
}

impl ClientMediaServer {
    pub fn new(config: &Config, base_url: &str) -> Self {
        Self {
            media_server: MediaServer::new(config),
            media_proxy_key: config.instance().ed25519_secret_key,
            base_url: base_url.to_string(),
        }
    }

    #[cfg(test)]
    pub fn for_test(base_url: &str) -> Self {
        use apx_core::crypto::eddsa::generate_weak_ed25519_key;
        let media_server = MediaServer::for_test(base_url);
        let media_proxy_key = generate_weak_ed25519_key();
        Self {
            media_server,
            media_proxy_key,
            base_url: base_url.to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn url_for(&self, media_info: &PartialMediaInfo) -> String {
        let file_name = match media_info {
            PartialMediaInfo::File { file_info, .. } => &file_info.file_name,
            PartialMediaInfo::Link { url, .. } => {
                let signature_base = url.as_bytes();
                let signature = create_eddsa_signature(
                    &self.media_proxy_key,
                    signature_base,
                );
                return format!(
                    "{}/api/media_proxy/{}?signature={}",
                    self.base_url,
                    url_encode(url),
                    hex::encode(signature),
                );
            },
        };
        match &self.media_server {
            MediaServer::Filesystem(backend) => {
                let mut media_server = backend.clone();
                media_server.override_base_url(&self.base_url);
                media_server.url_for(file_name)
            },
            MediaServer::S3(backend) => backend.url_for(file_name),
        }
    }
}
