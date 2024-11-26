use mitra_config::Config;
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

    pub fn url_for(&self, file_name: &str) -> String {
        let media_server = {
            let mut media_server = self.media_server.clone();
            media_server.override_base_url(&self.base_url);
            media_server
        };
        media_server.url_for(file_name)
    }
}
