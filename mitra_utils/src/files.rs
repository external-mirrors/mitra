use std::fmt;
use std::fs::{
    set_permissions,
    File,
    Permissions,
};
use std::io::Error;
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use mime_guess::get_mime_extensions_str;

pub fn get_media_type_extension(media_type: &str) -> Option<&'static str> {
    match media_type {
        // Override extension provided by mime_guess
        "image/jpeg" => Some("jpg"),
        _ => {
            get_mime_extensions_str(media_type)
                .and_then(|extensions| extensions.first())
                .copied()
        }
    }
}

pub fn write_file(data: &[u8], file_path: &Path) -> Result<(), Error> {
    let mut file = File::create(file_path)?;
    file.write_all(data)?;
    Ok(())
}

pub fn set_file_permissions(file_path: &Path, mode: u32) -> Result<(), Error> {
    let permissions = Permissions::from_mode(mode);
    set_permissions(file_path, permissions)?;
    Ok(())
}

#[derive(Debug)]
pub struct FileSize(usize);

impl FileSize {
    pub fn new(size: usize) -> Self {
        Self(size)
    }
}

impl fmt::Display for FileSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (value, unit) = match self.0 {
            size if size > 10_000_000_000 => (size / 1_000_000_000, "GB"),
            size if size > 10_000_000 => (size / 1_000_000, "MB"),
            size if size > 10_000 => (size / 1_000, "kB"),
            size => (size, "B"),
        };
        write!(formatter, "{}{}", value, unit)
    }
}

pub struct FileInfo {
    pub file_name: String,
    pub file_size: usize,
    pub digest: [u8; 32],
    pub media_type: String,
}

impl FileInfo {
    pub fn new(
        file_name: String,
        file_size: usize,
        digest: [u8; 32],
        media_type: String,
    ) -> Self {
        Self { file_name, file_size, digest, media_type }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_media_type_extension() {
        assert_eq!(
            get_media_type_extension("image/png"),
            Some("png"),
        );
        assert_eq!(
            get_media_type_extension("image/jpeg"),
            Some("jpg"),
        );
        assert_eq!(
            get_media_type_extension("image/avif"),
            Some("avif"),
        );
    }

    #[test]
    fn test_format_file_size() {
        let size = FileSize::new(9_999_000_000_000);
        assert_eq!(size.to_string(), "9999GB");
        let size = FileSize::new(10_123_000_000);
        assert_eq!(size.to_string(), "10GB");
        let size = FileSize::new(1_123_000_000);
        assert_eq!(size.to_string(), "1123MB");
        let size = FileSize::new(10_123_000);
        assert_eq!(size.to_string(), "10MB");
        let size = FileSize::new(123_000);
        assert_eq!(size.to_string(), "123kB");
        let size = FileSize::new(10_123);
        assert_eq!(size.to_string(), "10kB");
        let size = FileSize::new(123);
        assert_eq!(size.to_string(), "123B");
    }
}
