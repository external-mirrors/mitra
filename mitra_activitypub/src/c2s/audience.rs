use apx_sdk::core::url::{
    ap_uri::ApUri,
    canonical::with_gateway,
};
use mitra_config::Instance;
use mitra_models::accounts::types::ManagedAccount;

use crate::{
    actors::builders::local_actor_data,
    authority::Authority,
    deliverer::Recipient,
};

pub fn get_recipients(
    instance: &Instance,
    sender: &impl ManagedAccount,
) -> Vec<Recipient> {
    let authority = Authority::from(instance);
    let actor_data = local_actor_data(authority.root(), sender.profile());
    let outbox = actor_data.outbox;
    let outbox_ap = ApUri::parse(&outbox)
        .expect("outbox URI should be valid");
    let outbox_http = with_gateway(&outbox_ap, instance.uri_str());
    let recipient = Recipient::new(&actor_data.id, &outbox_http);
    vec![recipient]
}
