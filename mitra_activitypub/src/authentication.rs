use serde_json::{Value as JsonValue};

use apx_core::{
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
    http_digest::ContentDigest,
    http_signatures::{
        verify::{
            parse_http_signature,
            verify_http_signature,
            HttpSignatureVerificationError as HttpSignatureError,
        },
    },
    http_types::{
        HeaderMap,
        Method,
        Uri,
    },
    json_signatures::{
        proofs::ProofType,
        verify::{
            get_json_signature,
            verify_eddsa_json_signature,
            verify_rsa_json_signature,
            JsonSignatureVerificationError as JsonSignatureError,
            JsonSigner,
        },
    },
};
use apx_sdk::{
    authentication::{
        verify_portable_object,
        AuthenticationError as PortableObjectAuthenticationError,
    },
    deserialization::get_object_id,
    url::is_same_origin,
    utils::key_id_to_actor_id,
};
use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::get_remote_profile_by_actor_id,
    profiles::types::{
        DbActorProfile,
        PublicKeyType,
    },
};

use crate::{
    errors::HandlerError,
    identifiers::canonicalize_id,
    importers::{ActorIdResolver, ApClient},
};

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
    InvalidKeyId,

    #[error(transparent)]
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
    UnexpectedRequestSigner,

    #[error("actor and object signer do not match")]
    UnexpectedObjectSigner,

    #[error("invalid portable activity: {0}")]
    InvalidPortableActivity(#[from] PortableObjectAuthenticationError),
}

async fn get_signer(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    signer_id: &str,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signer = if no_fetch {
        // Avoid fetching (e.g. if signer was deleted)
        let canonical_signer_id = canonicalize_id(signer_id)
            .map_err(|_| AuthenticationError::ActorError("invalid actor ID"))?;
        get_remote_profile_by_actor_id(
            db_client,
            &canonical_signer_id.to_string(),
        ).await?
    } else {
        let mut ap_client = ApClient::new(config, db_client).await?;
        ap_client.instance.fetcher_timeout = AUTHENTICATION_FETCHER_TIMEOUT;
        match ActorIdResolver::default().only_remote().resolve(
            &ap_client,
            db_client,
            signer_id,
        ).await {
            Ok(profile) => profile,
            Err(HandlerError::DatabaseError(DatabaseError::NotFound(_))) => {
                let error_message = "signer not found in cache";
                return Err(AuthenticationError::ImportError(error_message.to_string()));
            },
            Err(HandlerError::DatabaseError(error)) => return Err(error.into()),
            Err(other_error) => {
                return Err(AuthenticationError::ImportError(other_error.to_string()));
            },
        }
    };
    assert!(!signer.is_local(), "signer should not be local actor");
    Ok(signer)
}

fn get_signer_ed25519_key(
    profile: &DbActorProfile,
    key_id: &str,
) -> Result<Ed25519PublicKey, AuthenticationError> {
    let canonical_key_id = canonicalize_id(key_id)
        .map_err(|_| AuthenticationError::ActorError("invalid key ID"))?;
    let actor_key = profile.public_keys
        .inner().iter()
        .find(|key| key.id == canonical_key_id.to_string())
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
    let canonical_key_id = canonicalize_id(key_id)
        .map_err(|_| AuthenticationError::ActorError("invalid key ID"))?;
    let maybe_actor_key = profile.public_keys
        .inner().iter()
        .find(|key| key.id == canonical_key_id.to_string());
    let rsa_public_key = if let Some(actor_key) = maybe_actor_key {
        if actor_key.key_type != PublicKeyType::RsaPkcs1 {
            return Err(AuthenticationError::ActorError("unexpected key type"));
        };
        rsa_public_key_from_pkcs1_der(&actor_key.key_data)?
    } else {
        // TODO: remove public_key from actor data
        log::warn!("key not found in public_keys: {}", canonical_key_id);
        let public_key = &profile.actor_json.as_ref()
            .expect("should be signed by remote actor")
            .public_key.as_ref()
            .filter(|public_key| public_key.id == key_id)
            .ok_or(AuthenticationError::ActorError("key not found"))?;
        deserialize_rsa_public_key(&public_key.public_key_pem)?
    };
    Ok(rsa_public_key)
}

/// Verifies HTTP signature and returns signer
pub async fn verify_signed_request(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    request_method: Method,
    request_uri: Uri,
    request_headers: HeaderMap,
    maybe_content_digest: Option<ContentDigest>,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signature_data = match parse_http_signature(
        &request_method,
        &request_uri,
        &request_headers,
    ) {
        Ok(signature_data) => signature_data,
        Err(HttpSignatureError::NoSignature) => {
            return Err(AuthenticationError::NoHttpSignature);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // TODO: FEP-EF61: support 'ap' URLs
    let signer_id = key_id_to_actor_id(signature_data.key_id.as_str())
        .map_err(|_| AuthenticationError::InvalidKeyId)?;
    let signer = get_signer(config, db_client, &signer_id, no_fetch).await?;
    let signer_key = get_signer_rsa_key(
        &signer,
        signature_data.key_id.as_str(),
    )?;

    verify_http_signature(
        &signature_data,
        &signer_key,
        maybe_content_digest,
    )?;

    Ok(signer)
}

/// Verifies JSON signature and returns signer
pub async fn verify_signed_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: &JsonValue,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    match verify_portable_object(activity) {
        Ok(activity_id) => {
            let actor_id = get_object_id(&activity["actor"])
                .map_err(|_| AuthenticationError::ActorError("unknown actor"))?;
            if !is_same_origin(&activity_id, &actor_id)
                .map_err(|_| AuthenticationError::ActorError("invalid actor ID"))?
            {
                return Err(AuthenticationError::UnexpectedObjectSigner);
            };
            let actor_profile = get_signer(config, db_client, &actor_id, no_fetch).await?;
            return Ok(actor_profile);
        },
        // Continue verification if activity is not portable
        Err(PortableObjectAuthenticationError::NotPortable) => (),
        Err(PortableObjectAuthenticationError::InvalidObjectID(message)) => {
            return Err(AuthenticationError::ActorError(message));
        },
        Err(other_error) => return Err(other_error.into()),
    };
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
    let actor_id = get_object_id(&activity["actor"])
        .map_err(|_| AuthenticationError::ActorError("unknown actor"))?;
    let actor_profile = get_signer(config, db_client, &actor_id, no_fetch).await?;

    match signature_data.signer {
        JsonSigner::HttpUrl(key_id) => {
            let signer_id = key_id_to_actor_id(key_id.as_str())
                .map_err(|_| AuthenticationError::InvalidKeyId)?;
            if signer_id != actor_id {
                return Err(AuthenticationError::UnexpectedObjectSigner);
            };
            match signature_data.proof_type {
                ProofType::JcsRsaSignature => {
                    let signer_key = get_signer_rsa_key(
                        &actor_profile,
                        key_id.as_str(),
                    )?;
                    verify_rsa_json_signature(
                        &signer_key,
                        &signature_data.object,
                        &signature_data.signature,
                    )?;
                },
                ProofType::JcsEddsaSignature | ProofType::EddsaJcsSignature => {
                    // Treat eddsa-jcs-2022 as a temporary alias
                    // for jcs-eddsa-2022 (no context injection)
                    let signer_key = get_signer_ed25519_key(
                        &actor_profile,
                        key_id.as_str(),
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
        JsonSigner::DidUrl(did) => {
            log::warn!("activity signed by {did}");
            return Err(AuthenticationError::InvalidJsonSignatureType);
        },
    };
    // Signer is actor
    Ok(actor_profile)
}
