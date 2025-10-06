pub fn get_user_feed_url(instance_uri: &str, username: &str) -> String {
    format!("{}/feeds/users/{}", instance_uri, username)
}
