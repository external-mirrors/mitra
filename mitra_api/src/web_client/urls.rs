/// URL builders for default frontend
use uuid::Uuid;

// Assuming frontend is on the same host as backend
pub fn get_profile_page_url(instance_uri: &str, username: &str) -> String {
    format!("{}/@{}", instance_uri, username)
}

pub fn get_post_page_url(instance_uri: &str, post_id: Uuid) -> String {
    format!("{}/post/{}", instance_uri, post_id)
}

pub fn get_tag_page_url(instance_uri: &str, tag_name: &str) -> String {
    format!("{}/tag/{}", instance_uri, tag_name)
}

pub fn get_subscription_page_url(instance_uri: &str, username: &str) -> String {
    format!(
        "{}/subscription",
        get_profile_page_url(instance_uri, username),
    )
}

pub fn get_search_page_url(instance_uri: &str, query: &str) -> String {
    format!("{instance_uri}/search?q={query}")
}

pub fn get_opengraph_image_url(instance_uri: &str) -> String {
    format!("{instance_uri}/ogp-image.png")
}
