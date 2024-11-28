use super::types::MediaInfo;

impl MediaInfo {
    pub fn png_for_test() -> Self {
        Self {
            file_name: "test.png".to_string(),
            file_size: 10000,
            digest: [0; 32],
            media_type: "image/png".to_string(),
            url: None,
        }
    }
}
