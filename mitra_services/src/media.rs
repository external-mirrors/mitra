use std::fs::remove_file;
use std::io::{Error as IoError};
use std::path::{Path, PathBuf};

use apx_core::hashes::sha256;
use thiserror::Error;

use mitra_config::Config;
use mitra_utils::{
    files::{
        get_media_type_extension,
        write_file,
        FileInfo,
        FileSize,
    },
    sysinfo::get_available_disk_space,
};

/// Generates unique file name based on file contents
fn get_file_name(data: &[u8], media_type: Option<&str>) -> String {
    let digest = sha256(data);
    let mut file_name = hex::encode(digest);
    let maybe_extension = media_type
        .and_then(get_media_type_extension);
    if let Some(extension) = maybe_extension {
        // Append extension for known media types
        file_name = format!("{}.{}", file_name, extension);
    };
    file_name
}

/// Save validated file to specified directory
fn save_file(
    data: Vec<u8>,
    output_dir: &Path,
    media_type: Option<&str>,
) -> Result<String, IoError> {
    let file_name = get_file_name(&data, media_type);
    let file_path = output_dir.join(&file_name);
    write_file(&data, &file_path)?;
    Ok(file_name)
}

#[derive(Debug, Error)]
pub enum MediaStorageError {
    #[error(transparent)]
    IoError(#[from] IoError),
}

trait MediaStorageBackend {
    fn save_file(
        &self,
        file_data: Vec<u8>,
        media_type: &str,
    ) -> Result<FileInfo, MediaStorageError>;

    fn read_file(
         &self,
        file_name: &str,
    ) -> Result<Vec<u8>, MediaStorageError>;

    fn delete_file(
        &self,
        file_name: &str,
    ) -> Result<(), MediaStorageError>;

    fn list_files(&self) -> Result<Vec<String>, MediaStorageError>;
}

#[derive(Clone)]
pub struct FilesystemStorage {
    pub media_dir: PathBuf,
}

impl FilesystemStorage {
    const MEDIA_DIR: &str = "media";

    pub fn init(&self) -> Result<(), MediaStorageError> {
        if !self.media_dir.exists() {
            std::fs::create_dir(&self.media_dir)?;
        };
        match get_available_disk_space(&self.media_dir) {
            Ok(amount) => {
                log::info!("available space: {}", FileSize::new(amount));
            },
            Err(error) => {
                log::warn!("failed to determine available space: {error}");
            },
        };
        Ok(())
    }
}

impl MediaStorageBackend for FilesystemStorage {
    fn save_file(
        &self,
        file_data: Vec<u8>,
        media_type: &str,
    ) -> Result<FileInfo, MediaStorageError> {
        let file_size = file_data.len();
        let digest = sha256(&file_data);
        let file_name = save_file(
            file_data,
            &self.media_dir,
            Some(media_type),
        )?;
        let file_info = FileInfo::new(
            file_name,
            file_size,
            digest,
            media_type.to_string(),
        );
        Ok(file_info)
    }

    fn read_file(
        &self,
        file_name: &str,
    ) -> Result<Vec<u8>, MediaStorageError> {
        let file_path = self.media_dir.join(file_name);
        let data = std::fs::read(file_path)?;
        Ok(data)
    }

    fn delete_file(
        &self,
        file_name: &str,
    ) -> Result<(), MediaStorageError> {
        let file_path = self.media_dir.join(file_name);
        remove_file(file_path)?;
        Ok(())
    }

    fn list_files(&self) -> Result<Vec<String>, MediaStorageError> {
        let mut files = vec![];
        for maybe_path in std::fs::read_dir(&self.media_dir)? {
            let file_name = maybe_path?.file_name()
                .to_string_lossy().to_string();
            files.push(file_name);
        };
        Ok(files)
    }
}

impl From<&Config> for FilesystemStorage {
    fn from(config: &Config) -> Self {
        Self {
            media_dir: config.storage_dir.join(Self::MEDIA_DIR),
        }
    }
}

#[derive(Clone)]
pub enum MediaStorage {
    Filesystem(FilesystemStorage),
}

impl MediaStorage {
    pub fn new(config: &Config) -> Self {
        let storage = FilesystemStorage::from(config);
        Self::Filesystem(storage)
    }

    fn backend(&self) -> &dyn MediaStorageBackend {
        match self {
            Self::Filesystem(backend) => backend,
        }
    }

    pub fn save_file(
        &self,
        file_data: Vec<u8>,
        media_type: &str,
    ) -> Result<FileInfo, MediaStorageError> {
        self.backend().save_file(file_data, media_type)
    }

    pub fn read_file(
        &self,
        file_name: &str,
    ) -> Result<Vec<u8>, MediaStorageError> {
        self.backend().read_file(file_name)
    }

    pub fn delete_file(
        &self,
        file_name: &str,
    ) -> Result<(), MediaStorageError> {
        self.backend().delete_file(file_name)
    }

    pub fn list_files(&self) -> Result<Vec<String>, MediaStorageError> {
        self.backend().list_files()
    }
}

#[derive(Clone)]
pub struct FilesystemServer {
    base_url: String,
}

impl FilesystemServer {
    pub const BASE_PATH: &str = "/media";

    pub fn new(base_url: &str) -> Self {
        Self { base_url: base_url.to_string() }
    }

    pub fn override_base_url(&mut self, base_url: &str) -> () {
        self.base_url = base_url.to_string();
    }

    // actix-files uses file name to guess content type
    // https://docs.rs/actix-files/0.6.6/actix_files/struct.NamedFile.html#method.from_file
    pub fn url_for(&self, file_name: &str) -> String {
        format!("{}{}/{}", self.base_url, Self::BASE_PATH, file_name)
    }
}

pub enum MediaServer {
    Filesystem(FilesystemServer),
}

impl MediaServer {
    pub fn new(config: &Config) -> Self {
        let backend = FilesystemServer::new(config.instance().uri_str());
        Self::Filesystem(backend)
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_test(base_url: &str) -> Self {
        let backend = FilesystemServer::new(base_url);
        Self::Filesystem(backend)
    }

    pub fn url_for(&self, file_name: &str) -> String {
        match self {
            Self::Filesystem(backend) => backend.url_for(file_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use apx_core::media_type::sniff_media_type;
    use super::*;

    #[test]
    fn test_get_file_name() {
        let mut data = vec![];
        data.extend_from_slice(b"\x89PNG\x0D\x0A\x1A\x0A");
        let media_type = sniff_media_type(&data);
        let file_name = get_file_name(&data, media_type.as_deref());

        assert_eq!(
            file_name,
            "4c4b6a3be1314ab86138bef4314dde022e600960d8689a2c8f8631802d20dab6.png",
        );
    }

    #[test]
    fn test_get_file_url() {
        let instance_uri = "https://social.example";
        let media_server = FilesystemServer::new(instance_uri);
        let file_name = "4c4b6a3be1314ab86138bef4314dde022e600960d8689a2c8f8631802d20dab6.png";
        let url = media_server.url_for(file_name);
        assert_eq!(
            url,
            "https://social.example/media/4c4b6a3be1314ab86138bef4314dde022e600960d8689a2c8f8631802d20dab6.png",
        );
    }
}
