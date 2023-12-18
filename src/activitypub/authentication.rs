use actix_web::HttpRequest;
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::get_profile_by_remote_actor_id,
    profiles::types::{
        DbActorProfile,
        PublicKeyType,
    },
};
use mitra_services::media::MediaStorage;
use mitra_utils::{
    crypto_eddsa::{
        ed25519_public_key_from_bytes,
        Ed25519PublicKey,
        Ed25519SerializationError,
    },
    crypto_rsa::{
        deserialize_rsa_public_key,
        rsa_public_key_from_pkcs1_der,
        RsaPublicKey,
        RsaSerializationError,
    },
    http_signatures::verify::{
        parse_http_signature,
        verify_http_signature,
        HttpSignatureVerificationError as HttpSignatureError,
    },
    json_signatures::{
        proofs::ProofType,
        verify::{
            get_json_signature,
            verify_blake2_ed25519_json_signature,
            verify_eddsa_json_signature,
            verify_eip191_json_signature,
            verify_rsa_json_signature,
            JsonSignatureVerificationError as JsonSignatureError,
            JsonSigner,
        },
    },
};

use super::deserialization::find_object_id;
use super::importers::get_or_import_profile_by_actor_id;
use super::receiver::HandlerError;

const AUTHENTICATION_FETCHER_TIMEOUT: u64 = 10;

#[derive(thiserror::Error, Debug)]
pub enum AuthenticationError {
    #[error(transparent)]
    HttpSignatureError(#[from] HttpSignatureError),

    #[error("no HTTP signature")]
    NoHttpSignature,

    #[error(transparent)]
    JsonSignatureError(#[from] JsonSignatureError),

    #[error("no JSON signature")]
    NoJsonSignature,

    #[error("invalid JSON signature type")]
    InvalidJsonSignatureType,

    #[error("invalid key ID")]
    InvalidKeyId(#[from] url::ParseError),

    #[error("database error")]
    DatabaseError(#[from] DatabaseError),

    #[error("{0}")]
    ImportError(String),

    #[error("{0}")]
    ActorError(&'static str),

    #[error("invalid RSA public key")]
    InvalidRsaPublicKey(#[from] RsaSerializationError),

    #[error("invalid Ed25519 public key")]
    InvalidEd25519PublicKey(#[from] Ed25519SerializationError),

    #[error("actor and request signer do not match")]
    UnexpectedSigner,
}

fn key_id_to_actor_id(key_id: &str) -> Result<String, AuthenticationError> {
    let key_url = url::Url::parse(key_id)?;
    if key_url.query().is_some() {
        log::warn!("key ID contains query string: {}", key_id);
    };
    // Strip #main-key (works with most AP servers)
    let actor_id = &key_url[..url::Position::BeforeQuery];
    // GoToSocial compat
    let actor_id = actor_id.trim_end_matches("/main-key");
    Ok(actor_id.to_string())
}

async fn get_signer(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    signer_id: &str,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signer = if no_fetch {
        // Avoid fetching (e.g. if signer was deleted)
        get_profile_by_remote_actor_id(db_client, signer_id).await?
    } else {
        let mut instance = config.instance();
        instance.fetcher_timeout = AUTHENTICATION_FETCHER_TIMEOUT;
        match get_or_import_profile_by_actor_id(
            db_client,
            &instance,
            &MediaStorage::from(config),
            signer_id,
        ).await {
            Ok(profile) => profile,
            Err(HandlerError::DatabaseError(error)) => return Err(error.into()),
            Err(other_error) => {
                return Err(AuthenticationError::ImportError(other_error.to_string()));
            },
        }
    };
    Ok(signer)
}

fn get_signer_ed25519_key(
    profile: &DbActorProfile,
    key_id: &str,
) -> Result<Ed25519PublicKey, AuthenticationError> {
    let actor_key = profile.public_keys
        .inner().iter()
        .find(|key| key.id == key_id)
        .ok_or(AuthenticationError::ActorError("key not found"))?;
    if actor_key.key_type != PublicKeyType::Ed25519 {
        return Err(AuthenticationError::ActorError("unexpected key type"));
    };
    let ed25519_public_key = ed25519_public_key_from_bytes(&actor_key.key_data)?;
    Ok(ed25519_public_key)
}

fn get_signer_rsa_key(
    profile: &DbActorProfile,
    key_id: &str,
) -> Result<RsaPublicKey, AuthenticationError> {
    let maybe_actor_key = profile.public_keys
        .inner().iter()
        .find(|key| key.id == key_id);
    let rsa_public_key = if let Some(actor_key) = maybe_actor_key {
        if actor_key.key_type != PublicKeyType::RsaPkcs1 {
            return Err(AuthenticationError::ActorError("unexpected key type"));
        };
        rsa_public_key_from_pkcs1_der(&actor_key.key_data)?
    } else {
        let public_key_pem = &profile.actor_json.as_ref()
            .expect("should be signed by remote actor")
            .public_key
            .public_key_pem;
        deserialize_rsa_public_key(public_key_pem)?
    };
    Ok(rsa_public_key)
}

/// Verifies HTTP signature and returns signer
pub async fn verify_signed_request(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    request: &HttpRequest,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signature_data = match parse_http_signature(
        request.method(),
        request.uri(),
        request.headers(),
    ) {
        Ok(signature_data) => signature_data,
        Err(HttpSignatureError::NoSignature) => {
            return Err(AuthenticationError::NoHttpSignature);
        },
        Err(other_error) => return Err(other_error.into()),
    };

    let signer_id = key_id_to_actor_id(&signature_data.key_id)?;
    let signer = get_signer(config, db_client, &signer_id, no_fetch).await?;
    let signer_key = get_signer_rsa_key(
        &signer,
        &signature_data.key_id,
    )?;

    verify_http_signature(&signature_data, &signer_key)?;

    Ok(signer)
}

/// Verifies JSON signature and returns signer
pub async fn verify_signed_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: &JsonValue,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signature_data = match get_json_signature(activity) {
        Ok(signature_data) => signature_data,
        Err(JsonSignatureError::NoProof) => {
            return Err(AuthenticationError::NoJsonSignature);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // Signed activities must have `actor` property, to avoid situations
    // where signer is identified by DID but there is no matching
    // identity proof in the local database.
    let actor_id = find_object_id(&activity["actor"])
        .map_err(|_| AuthenticationError::ActorError("unknown actor"))?;
    let actor_profile = get_signer(config, db_client, &actor_id, no_fetch).await?;

    match signature_data.signer {
        JsonSigner::ActorKeyId(ref key_id) => {
            let signer_id = key_id_to_actor_id(key_id)?;
            if signer_id != actor_id {
                return Err(AuthenticationError::UnexpectedSigner);
            };
            match signature_data.proof_type {
                ProofType::JcsRsaSignature => {
                    let signer_key = get_signer_rsa_key(
                        &actor_profile,
                        key_id,
                    )?;
                    verify_rsa_json_signature(
                        &signer_key,
                        &signature_data.object,
                        &signature_data.signature,
                    )?;
                },
                ProofType::JcsEddsaSignature => {
                    let signer_key = get_signer_ed25519_key(
                        &actor_profile,
                        key_id,
                    )?;
                    verify_eddsa_json_signature(
                        &signer_key,
                        &signature_data.object,
                        &signature_data.proof_config,
                        &signature_data.signature,
                    )?;
                },
                _ => return Err(AuthenticationError::InvalidJsonSignatureType),
            };
        },
        JsonSigner::Did(did) => {
            if !actor_profile.identity_proofs.any(&did) {
                return Err(AuthenticationError::UnexpectedSigner);
            };
            match signature_data.proof_type {
                ProofType::JcsBlake2Ed25519Signature => {
                    let did_key = did.as_did_key()
                        .ok_or(AuthenticationError::InvalidJsonSignatureType)?;
                    verify_blake2_ed25519_json_signature(
                        did_key,
                        &signature_data.object,
                        &signature_data.signature,
                    )?;
                },
                ProofType::JcsEip191Signature => {
                    let did_pkh = did.as_did_pkh()
                        .ok_or(AuthenticationError::InvalidJsonSignatureType)?;
                    verify_eip191_json_signature(
                        did_pkh,
                        &signature_data.object,
                        &signature_data.signature,
                    )?;
                },
                _ => return Err(AuthenticationError::InvalidJsonSignatureType),
            };
        },
    };
    // Signer is actor
    Ok(actor_profile)
}

pub fn verify_signed_c2s_activity(
    actor_profile: &DbActorProfile,
    activity: &JsonValue,
) -> Result<(), AuthenticationError> {
    let signature_data = match get_json_signature(activity) {
        Ok(signature_data) => signature_data,
        Err(JsonSignatureError::NoProof) => {
            return Err(AuthenticationError::NoJsonSignature);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    match signature_data.signer {
        JsonSigner::Did(did) => {
            if !actor_profile.identity_proofs.any(&did) {
                return Err(AuthenticationError::UnexpectedSigner);
            };
            match signature_data.proof_type {
                ProofType::JcsEip191Signature => {
                    let did_pkh = did.as_did_pkh()
                        .ok_or(AuthenticationError::InvalidJsonSignatureType)?;
                    verify_eip191_json_signature(
                        did_pkh,
                        &signature_data.object,
                        &signature_data.signature,
                    )?;
                },
                _ => return Err(AuthenticationError::InvalidJsonSignatureType),
            };
        },
        _ => return Err(AuthenticationError::InvalidJsonSignatureType),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_id_to_actor_id() {
        let key_id = "https://myserver.org/actor#main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://myserver.org/actor");

        let key_id = "https://myserver.org/actor/main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://myserver.org/actor");
    }
}
