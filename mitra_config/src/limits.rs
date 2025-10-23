use regex::Regex;
use serde::{
    Deserialize,
    Deserializer,
    de::{Error as DeserializerError},
};
use super::ConfigError;

// Not included
// https://developer.mozilla.org/en-US/docs/Web/Media/Formats/Image_types
// - image/tiff only supported by Safari
const SUPPORTED_MEDIA_TYPES: [&str; 20] = [
    "audio/flac",
    "audio/mpeg",
    "audio/ogg",
    "audio/x-wav",
    "audio/wav",
    "audio/vnd.wave",
    "audio/mp4",
    "audio/aac",
    "audio/x-m4a",
    "image/apng",
    "image/avif",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/webp",
    "video/mp4",
    "video/ogg",
    "video/quicktime",
    "video/webm",
    "video/x-m4v",
];

const FILE_SIZE_RE: &str = r"^(?i)(?P<size>\d+)(?P<unit>[kmg]?)b?$";

fn parse_file_size(value: &str) -> Result<usize, ConfigError> {
    let file_size_re = Regex::new(FILE_SIZE_RE)
        .expect("regexp should be valid");
    let caps = file_size_re.captures(value)
        .ok_or(ConfigError("invalid file size"))?;
    let size: usize = caps["size"].to_string().parse()
        .map_err(|_| ConfigError("invalid file size"))?;
    let unit = caps["unit"].to_string().to_lowercase();
    let multiplier = match unit.as_str() {
        "k" => usize::pow(10, 3),
        "m" => usize::pow(10, 6),
        "g" => usize::pow(10, 9),
        "" => 1,
        _ => return Err(ConfigError("invalid file size unit")),
    };
    Ok(size * multiplier)
}

fn deserialize_file_size<'de, D>(
    deserializer: D,
) -> Result<usize, D::Error>
    where D: Deserializer<'de>
{
    let file_size_str = String::deserialize(deserializer)?;
    let file_size = parse_file_size(&file_size_str)
        .map_err(DeserializerError::custom)?;
    Ok(file_size)
}

const fn default_file_size_limit() -> usize { 20_000_000 } // 20 MB

const fn default_profile_image_size_limit() -> usize { 5_000_000 } // 5 MB
// https://github.com/mastodon/mastodon/blob/v4.3.3/app/models/concerns/account/avatar.rb
const fn default_profile_image_local_size_limit() -> usize { 2_000_000 } // 2 MB

const fn default_emoji_size_limit() -> usize { 1_000_000 } // 1 MB
// https://github.com/mastodon/mastodon/blob/v4.2.8/app/models/custom_emoji.rb#L27
const fn default_emoji_local_size_limit() -> usize { 256_000 } // 256 kB

#[derive(Clone, Deserialize)]
pub struct MediaLimits {
    #[serde(
        default = "default_file_size_limit",
        deserialize_with = "deserialize_file_size",
    )]
    pub file_size_limit: usize,

    #[serde(
        default = "default_profile_image_size_limit",
        deserialize_with = "deserialize_file_size",
    )]
    pub profile_image_size_limit: usize,

    #[serde(
        default = "default_profile_image_local_size_limit",
        deserialize_with = "deserialize_file_size",
    )]
    pub profile_image_local_size_limit: usize,

    #[serde(
        default = "default_emoji_size_limit",
        deserialize_with = "deserialize_file_size",
    )]
    pub emoji_size_limit: usize,

    #[serde(
        default = "default_emoji_local_size_limit",
        deserialize_with = "deserialize_file_size",
    )]
    pub emoji_local_size_limit: usize,

    // Add items to the list of supported media types
    #[serde(default)]
    extra_supported_types: Vec<String>,
}

impl Default for MediaLimits {
    fn default() -> Self {
        Self {
            file_size_limit: default_file_size_limit(),
            profile_image_size_limit: default_profile_image_size_limit(),
            profile_image_local_size_limit: default_profile_image_local_size_limit(),
            emoji_size_limit: default_emoji_size_limit(),
            emoji_local_size_limit: default_emoji_local_size_limit(),
            extra_supported_types: vec![],
        }
    }
}

impl MediaLimits {
    pub fn supported_media_types(&self) -> Vec<&str> {
        SUPPORTED_MEDIA_TYPES.into_iter()
            .chain(self.extra_supported_types.iter()
                .map(|media_type| media_type.as_str()))
            .collect()
    }
}

const fn default_post_character_limit() -> usize { 5000 }
const fn default_attachment_limit() -> usize { 16 }
// Mastodon's limit is 4
// https://github.com/mastodon/mastodon/blob/v4.3.7/app/models/status.rb#L42
const fn default_attachment_local_limit() -> usize { default_attachment_limit() }

#[derive(Clone, Deserialize)]
pub struct PostLimits {
    #[serde(default = "default_post_character_limit")]
    pub character_limit: usize,
    #[serde(default = "default_attachment_limit")]
    pub attachment_limit: usize,
    #[serde(default = "default_attachment_local_limit")]
    pub attachment_local_limit: usize,
}

impl Default for PostLimits {
    fn default() -> Self {
        Self {
            character_limit: default_post_character_limit(),
            attachment_limit: default_attachment_limit(),
            attachment_local_limit: default_attachment_local_limit(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct Limits {
    #[serde(default)]
    pub media: MediaLimits,
    #[serde(default)]
    pub posts: PostLimits,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_size() {
        let file_size = parse_file_size("1234").unwrap();
        assert_eq!(file_size, 1234);
        let file_size = parse_file_size("89kB").unwrap();
        assert_eq!(file_size, 89_000);
        let file_size = parse_file_size("12M").unwrap();
        assert_eq!(file_size, 12_000_000);
    }
}
