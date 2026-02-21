use serde::Deserialize;

const fn default_extraneous_posts() -> Option<u32> { Some(15) }
const fn default_empty_profiles() -> Option<u32> { Some(30) }
const fn default_activitypub_objects() -> Option<u32> { Some(5) }

// `None` disables pruning (not supported in TOML)
#[derive(Clone, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_extraneous_posts")]
    pub extraneous_posts: Option<u32>,
    #[serde(default = "default_empty_profiles")]
    pub empty_profiles: Option<u32>,
    #[serde(default = "default_activitypub_objects")]
    pub activitypub_objects: Option<u32>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            extraneous_posts: default_extraneous_posts(),
            empty_profiles: default_empty_profiles(),
            activitypub_objects: default_activitypub_objects(),
        }
    }
}
