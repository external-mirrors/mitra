use mitra_utils::files::FileInfo;

use super::types::MediaInfo;

impl MediaInfo {
    pub fn png_for_test() -> Self {
        let file_info = FileInfo {
            file_name: "test.png".to_string(),
            file_size: 10000,
            digest: [0; 32],
            media_type: "image/png".to_string(),
        };
        Self::local(file_info)
    }
}
