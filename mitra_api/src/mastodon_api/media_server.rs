use mitra_config::Config;
use mitra_models::media::types::PartialMediaInfo;
use mitra_services::media::MediaServer;

pub struct ClientMediaServer {
    media_server: MediaServer,
    base_url: String,
}

impl ClientMediaServer {
    pub fn new(config: &Config, base_url: &str) -> Self {
        Self {
            media_server: MediaServer::new(config),
            base_url: base_url.to_string(),
        }
    }

    #[cfg(test)]
    pub fn for_test(base_url: &str) -> Self {
        let media_server = MediaServer::for_test(base_url);
        Self {
            media_server,
            base_url: base_url.to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn url_for(&self, media_info: &PartialMediaInfo) -> String {
        let file_name = match media_info {
            PartialMediaInfo::File { file_info, .. } => &file_info.file_name,
            // TODO: use media proxy
            PartialMediaInfo::Link { url, .. } => return url.to_owned(),
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
