use apx_sdk::{
    agent::{FederationAgent, RequestSigner},
    core::{
        crypto_eddsa::{
            ed25519_public_key_from_secret_key,
            generate_ed25519_key,
        },
        crypto_rsa::generate_rsa_key,
        did_key::DidKey,
        json_signatures::create::sign_object_eddsa,
    },
    deliver::send_object,
};
use serde_json::json;

#[tokio::main]
async fn main() -> () {
    let identity_key = generate_ed25519_key();
    let identity_public_key = ed25519_public_key_from_secret_key(&identity_key);
    let authority = DidKey::from_ed25519_key(&identity_public_key);
    let request_signer = RequestSigner {
        key: generate_rsa_key().unwrap(),
        key_id: format!("http://127.0.0.1:8380/.well-known/apgateway/{authority}/rsa_key"),
    };
    let agent = FederationAgent {
        user_agent: Some("fep-ae97-client".to_string()),
        ssrf_protection_enabled: false, // allow connections to 127.0.0.1
        response_size_limit: 2_000_000,
        fetcher_timeout: 60,
        deliverer_timeout: 60,
        proxy_url: None,
        onion_proxy_url: None,
        i2p_proxy_url: None,
        signer: Some(request_signer),
    };
    let note = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("http://127.0.0.1:8380/.well-known/apgateway/{authority}/note/1"),
        "type": "Note",
        "attributedTo": format!("http://127.0.0.1:8380/.well-known/apgateway/{authority}/actor"),
        "content": "<p>test</p>",
    });
    let signed_note = sign_object_eddsa(
        &identity_key,
        &authority.to_string(),
        &note,
        None,
        false,
        true,
    ).unwrap();
    let create = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("http://127.0.0.1:8380/.well-known/apgateway/{authority}/activity/1"),
        "type": "Create",
        "actor": format!("http://127.0.0.1:8380/.well-known/apgateway/{authority}/actor"),
        "object": signed_note,
    });
    let signed_create = sign_object_eddsa(
        &identity_key,
        &authority.to_string(),
        &create,
        None,
        false,
        true,
    ).unwrap();
    send_object(
        &agent,
        &signed_create.to_string(),
        &format!("http://127.0.0.1:8380/.well-known/apgateway/{authority}/outbox"),
        &[],
    ).await.unwrap();
}
