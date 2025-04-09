use serde::Deserialize;

const fn default_extraneous_posts_retention() -> Option<u32> { Some(15) }
const fn default_empty_profiles_retention() -> Option<u32> { Some(30) }

#[derive(Clone, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_extraneous_posts_retention")]
    pub extraneous_posts: Option<u32>,
    #[serde(default = "default_empty_profiles_retention")]
    pub empty_profiles: Option<u32>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            extraneous_posts: default_extraneous_posts_retention(),
            empty_profiles: default_empty_profiles_retention(),
        }
    }
}
