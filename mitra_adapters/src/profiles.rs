use apx_sdk::addresses::WebfingerAddress;
use mitra_models::profiles::types::{DbActorProfile, WebfingerHostname};

pub fn profile_address(
    local_hostname: &str,
    profile: &DbActorProfile,
) -> Option<WebfingerAddress> {
    let maybe_hostname = profile.webfinger_hostname();
    let hostname = match maybe_hostname {
        WebfingerHostname::Remote(ref hostname) => hostname,
        WebfingerHostname::Local => local_hostname,
        WebfingerHostname::Unknown => return None,
    };
    let address = WebfingerAddress::new_unchecked(&profile.username, hostname);
    Some(address)
}
