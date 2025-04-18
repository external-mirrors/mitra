pub fn get_user_feed_url(instance_url: &str, username: &str) -> String {
    format!("{}/feeds/users/{}", instance_url, username)
}
