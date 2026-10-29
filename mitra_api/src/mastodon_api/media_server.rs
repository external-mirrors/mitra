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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_media_proxy_url() {
        let server = ClientMediaServer::for_test("https://local.example");
        let media_info = PartialMediaInfo::Link {
            media_type: "image/png".to_owned(),
            url: "https://remote.example/test".to_owned(),
        };
        let proxy_url = server.url_for(&media_info);
        assert_eq!(
            proxy_url,
            "https://local.example/api/media_proxy/https%3A%2F%2Fremote.example%2Ftest?signature=1a61132cceff1dd2f9adbcc8b2a840994374782e45defefdfab51670ca47c135574a9d47745fbab595bf43f4c1da811d7ed4506f12983ee71262187243156303",
        );
    }
}
