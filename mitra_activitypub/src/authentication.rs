use apx_core::{
    crypto::{
        common::PublicKey,
        eddsa::{
            ed25519_public_key_from_bytes,
            Ed25519PublicKey,
            Ed25519SerializationError,
        },
        rsa::{
            deserialize_rsa_public_key,
            rsa_public_key_from_pkcs1_der,
            RsaPublicKey,
            RsaSerializationError,
        },
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
            VerificationMethod,
        },
    },
};
use apx_sdk::{
    authentication::{
        verify_portable_object,
        AuthenticationError as PortableObjectAuthenticationError,
    },
    deserialization::object_to_id,
    utils::key_id_to_actor_id,
};
use serde_json::{Value as JsonValue};

use mitra_config::Config;
use mitra_models::{
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    profiles::queries::get_remote_profile_by_actor_id,
    profiles::types::{
        DbActorProfile,
        PublicKeyType,
    },
};
use mitra_validators::errors::ValidationError;

use crate::{
    errors::HandlerError,
    identifiers::canonicalize_id,
    importers::{ActorIdResolver, ApClient},
    ownership::{get_object_id, is_same_origin},
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

    #[error("unsupported verification method")]
    UnsupportedVerificationMethod,

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error("{0}")]
    ImportError(String),

    #[error("{0}")]
    ActorError(&'static str),

    #[error("can't retrieve key")]
    KeyRetrievalError(&'static str),

    #[error("invalid RSA public key")]
    InvalidRsaPublicKey(#[from] RsaSerializationError),

    #[error("invalid Ed25519 public key")]
    InvalidEd25519PublicKey(#[from] Ed25519SerializationError),

    #[error("actor and request signer do not match")]
    UnexpectedRequestSigner,

    #[error("actor and object signer do not match")]
    UnexpectedObjectSigner,

    #[error("object ID and verification method have different origins")]
    UnexpectedKeyOrigin,

    #[error("invalid portable activity: {0}")]
    InvalidPortableActivity(#[from] PortableObjectAuthenticationError),
}

async fn get_signer(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    signer_id: &str,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let signer = if no_fetch {
        // Avoid fetching (e.g. if signer was deleted)
        let canonical_signer_id = canonicalize_id(signer_id)
            .map_err(|_| ValidationError("invalid actor ID"))?;
        get_remote_profile_by_actor_id(
            db_client,
            &canonical_signer_id.to_string(),
        ).await?
    } else {
        let mut ap_client = ApClient::new(config, db_client).await?;
        ap_client.instance.federation.fetcher_timeout = AUTHENTICATION_FETCHER_TIMEOUT;
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

fn get_signer_key(
    profile: &DbActorProfile,
    key_id: &str,
) -> Result<PublicKey, AuthenticationError> {
    let canonical_key_id = canonicalize_id(key_id)
        .map_err(|_| ValidationError("invalid key ID"))?;
    let maybe_actor_key = profile.public_keys
        .inner().iter()
        .find(|key| {
            key.id == canonical_key_id.to_string()
            // Workaround for PeerTube
            // https://github.com/Chocobozzz/PeerTube/issues/6829
            || key.id == format!("{canonical_key_id}#main-key")
        });
    let public_key = if let Some(actor_key) = maybe_actor_key {
        match actor_key.key_type {
            PublicKeyType::RsaPkcs1 => {
                let public_key =
                    rsa_public_key_from_pkcs1_der(&actor_key.key_data)?;
                PublicKey::Rsa(public_key)
            },
            PublicKeyType::Ed25519 => {
                let public_key =
                    ed25519_public_key_from_bytes(&actor_key.key_data)?;
                PublicKey::Ed25519(public_key)
            },
        }
    } else {
        // TODO: remove public_key from actor data
        log::warn!("key not found in public_keys: {}", canonical_key_id);
        let public_key = &profile.actor_json.as_ref()
            .expect("should be signed by remote actor")
            .public_key.as_ref()
            .filter(|public_key| public_key.id == key_id)
            .ok_or(AuthenticationError::ActorError("key not found"))?;
        let public_key =
            deserialize_rsa_public_key(&public_key.public_key_pem)?;
        PublicKey::Rsa(public_key)
    };
    Ok(public_key)
}

fn get_signer_ed25519_key(
    profile: &DbActorProfile,
    key_id: &str,
) -> Result<Ed25519PublicKey, AuthenticationError> {
    let public_key = get_signer_key(profile, key_id)?;
    let PublicKey::Ed25519(ed25519_public_key) = public_key else {
        return Err(AuthenticationError::ActorError("unexpected key type"));
    };
    Ok(ed25519_public_key)
}

fn get_signer_rsa_key(
    profile: &DbActorProfile,
    key_id: &str,
) -> Result<RsaPublicKey, AuthenticationError> {
    let public_key = get_signer_key(profile, key_id)?;
    let PublicKey::Rsa(rsa_public_key) = public_key else {
        return Err(AuthenticationError::ActorError("unexpected key type"));
    };
    Ok(rsa_public_key)
}

/// Verifies HTTP signature and returns signer
pub async fn verify_signed_request(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    request_method: Method,
    request_uri: Uri,
    request_headers: HeaderMap,
    maybe_content_digest: Option<ContentDigest>,
    no_fetch: bool,
) -> Result<(VerificationMethod, DbActorProfile), AuthenticationError> {
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
    if signature_data.is_rfc9421 {
        log::info!("RFC-9421 signature found");
    };
    // Reciprocal claim on actor is required
    // https://codeberg.org/fediverse/fep/src/branch/main/fep/fe34/fep-fe34.md#signatures
    let signer_id = match signature_data.key_id {
        VerificationMethod::HttpUri(ref key_id) => {
            key_id_to_actor_id(key_id.as_str())
                .map_err(|_| ValidationError("invalid key ID"))?
        },
        VerificationMethod::ApUri(ref key_id) => {
            log::info!("request signed with {key_id}");
            key_id.without_fragment().to_string()
        },
        _ => return Err(AuthenticationError::UnsupportedVerificationMethod),
    };
    let signer = get_signer(config, db_pool, &signer_id, no_fetch).await?;
    let key_id = signature_data.key_id.to_string();
    let public_key = get_signer_key(
        &signer,
        key_id.as_str(),
    )?;
    if matches!(public_key, PublicKey::Ed25519(_)) {
        log::info!("Ed25519 key found");
    };

    verify_http_signature(
        &signature_data,
        &public_key,
        maybe_content_digest,
    )?;

    Ok((signature_data.key_id, signer))
}

/// Verifies JSON signature on activity and returns actor
pub async fn verify_signed_activity(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
    activity: &JsonValue,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    match verify_portable_object(activity) {
        Ok(canonical_activity_id) => {
            // Using actor-based verification because
            // an actor profile needs to be returned
            let actor_id = object_to_id(&activity["actor"])
                .map_err(|_| ValidationError("unknown actor"))?;
            let actor_profile = get_signer(config, db_pool, &actor_id, no_fetch).await?;
            let canonical_actor_id =
                canonicalize_id(actor_profile.expect_remote_actor_id())?;
            if canonical_activity_id.origin() != canonical_actor_id.origin() {
                return Err(AuthenticationError::UnexpectedObjectSigner);
            };
            return Ok(actor_profile);
        },
        // Continue verification if activity is not portable
        Err(PortableObjectAuthenticationError::NotPortable) => (),
        Err(PortableObjectAuthenticationError::InvalidObjectID(message)) => {
            return Err(ValidationError(message).into());
        },
        Err(other_error) => return Err(other_error.into()),
    };

    let activity_id = get_object_id(activity)?;
    let signature_data = match get_json_signature(activity) {
        Ok(signature_data) => signature_data,
        Err(JsonSignatureError::NoProof) => {
            return Err(AuthenticationError::NoJsonSignature);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    let actor_id = object_to_id(&activity["actor"])
        .map_err(|_| ValidationError("unknown actor"))?;
    let actor_profile = get_signer(config, db_pool, &actor_id, no_fetch).await?;

    match signature_data.verification_method {
        VerificationMethod::HttpUri(key_id) => {
            // Can this activity be signed with this key?
            if !is_same_origin(activity_id, key_id.as_str())? {
                return Err(AuthenticationError::UnexpectedKeyOrigin);
            };
            // Can this actor perform this activity?
            let signer_id = key_id_to_actor_id(key_id.as_str())
                .map_err(|_| ValidationError("invalid key ID"))?;
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
        VerificationMethod::ApUri(ap_uri) => {
            log::warn!("activity signed by {}", ap_uri);
            return Err(AuthenticationError::UnsupportedVerificationMethod);
        },
        VerificationMethod::DidUrl(did_url) => {
            log::warn!("activity signed by {}", did_url.did());
            return Err(AuthenticationError::UnsupportedVerificationMethod);
        },
    };
    // Signer is actor
    Ok(actor_profile)
}
