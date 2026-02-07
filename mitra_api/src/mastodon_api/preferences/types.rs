use serde::Serialize;

use mitra_models::users::types::SharedClientConfig;

use crate::mastodon_api::statuses::types::visibility_to_str;

// https://docs.joinmastodon.org/methods/preferences/#response
#[derive(Serialize)]
pub struct Preferences {
    #[serde(rename = "posting:default:visibility")]
    posting_default_visibility: &'static str,
    #[serde(rename = "posting:default:sensitive")]
    posting_default_sensitive: bool,
    #[serde(rename = "posting:default:language")]
    posting_default_language: Option<String>,
    #[serde(rename = "reading:expand:media")]
    reading_expand_media: &'static str,
    #[serde(rename = "reading:expand:spoilers")]
    reading_expand_spoilers: bool,
}

impl Preferences {
    pub fn new(client_config: SharedClientConfig) -> Self {
        Self {
            posting_default_visibility:
                visibility_to_str(client_config.default_post_visibility),
            posting_default_sensitive: false,
            posting_default_language: client_config.default_post_language
                .and_then(|language| language.inner().to_639_1())
                .map(|code| code.to_owned()),
            reading_expand_media: "default",
            reading_expand_spoilers: false,
        }
    }
}
