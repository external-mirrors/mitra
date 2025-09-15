use apx_sdk::{
    agent::FederationAgent,
    core::{
        crypto_eddsa::{
            ed25519_public_key_from_secret_key,
            generate_ed25519_key,
        },
        crypto_rsa::generate_rsa_key,
        did_key::DidKey,
        http_signatures::create::HttpSigner,
        json_signatures::create::sign_object,
    },
    deliver::send_object,
};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> () {
    let identity_key = generate_ed25519_key();
    let identity_public_key = ed25519_public_key_from_secret_key(&identity_key);
    let did = DidKey::from_ed25519_key(&identity_public_key);
    let http_key = generate_rsa_key().unwrap();
    let http_key_id = format!("http://127.0.0.1:8380/.well-known/apgateway/{did}/rsa_key");
    let http_signer = HttpSigner::new_rsa(http_key, http_key_id);
    let agent = FederationAgent {
        user_agent: Some("fep-ae97-client".to_string()),
        ssrf_protection_enabled: false, // allow connections to 127.0.0.1
        signer: Some(http_signer),
        ..Default::default()
    };
    let note = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("http://127.0.0.1:8380/.well-known/apgateway/{did}/note/1"),
        "type": "Note",
        "attributedTo": format!("http://127.0.0.1:8380/.well-known/apgateway/{did}/actor"),
        "content": "<p>test</p>",
    });
    let signed_note = sign_object(
        &identity_key,
        &did.verification_method_id(),
        &note,
    ).unwrap();
    let create = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("http://127.0.0.1:8380/.well-known/apgateway/{did}/activity/1"),
        "type": "Create",
        "actor": format!("http://127.0.0.1:8380/.well-known/apgateway/{did}/actor"),
        "object": signed_note,
    });
    let signed_create = sign_object(
        &identity_key,
        &did.verification_method_id(),
        &create,
    ).unwrap();
    send_object(
        &agent,
        &format!("http://127.0.0.1:8380/.well-known/apgateway/{did}/outbox"),
        &signed_create,
        &[],
    ).await.unwrap();
}
