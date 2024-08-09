use actix_web::HttpRequest;
use serde_json::{Value as JsonValue};

use mitra_activitypub::{
    errors::HandlerError,
    identifiers::canonicalize_id,
    importers::ActorIdResolver,
};
use mitra_config::Config;
use mitra_federation::{
    authentication::{
        verify_portable_object,
        AuthenticationError as PortableObjectAuthenticationError,
    },
    deserialization::get_object_id,
    url::is_same_origin,
    utils::key_id_to_actor_id,
};
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::get_remote_profile_by_actor_id,
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
    UnexpectedSigner,

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
            &canonical_signer_id,
        ).await?
    } else {
        let mut instance = config.instance();
        instance.fetcher_timeout = AUTHENTICATION_FETCHER_TIMEOUT;
        match ActorIdResolver::default().only_remote().resolve(
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
        .find(|key| key.id == canonical_key_id)
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
        .find(|key| key.id == canonical_key_id);
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
    request: &HttpRequest,
    content_digest: [u8; 32],
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
    // TODO: FEP-EF61: support 'ap' URLs
    let signer_id = key_id_to_actor_id(&signature_data.key_id)
        .map_err(|_| AuthenticationError::InvalidKeyId)?;
    let signer = get_signer(config, db_client, &signer_id, no_fetch).await?;
    let signer_key = get_signer_rsa_key(
        &signer,
        &signature_data.key_id,
    )?;

    verify_http_signature(
        &signature_data,
        &signer_key,
        Some(content_digest),
    )?;

    Ok(signer)
}

pub async fn verify_signed_get_request(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    request: &HttpRequest,
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
    // TODO: FEP-EF61: support 'ap' URLs
    let signer_id = key_id_to_actor_id(&signature_data.key_id)
        .map_err(|_| AuthenticationError::InvalidKeyId)?;
    let canonical_signer_id = canonicalize_id(&signer_id)
        .map_err(|_| AuthenticationError::InvalidKeyId)?;
    let signer = get_signer(
        config,
        db_client,
        &canonical_signer_id,
        true, // don't fetch
    ).await?;
    let canonical_key_id = canonicalize_id(&signature_data.key_id)
        .map_err(|_| AuthenticationError::InvalidKeyId)?;
    let signer_key = get_signer_rsa_key(
        &signer,
        &canonical_key_id,
    )?;

    verify_http_signature(
        &signature_data,
        &signer_key,
        None,
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
                return Err(AuthenticationError::UnexpectedSigner);
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
        JsonSigner::ActorKeyId(ref key_id) => {
            let signer_id = key_id_to_actor_id(key_id)
                .map_err(|_| AuthenticationError::InvalidKeyId)?;
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
                ProofType::JcsEddsaSignature | ProofType::EddsaJcsSignature => {
                    // Treat eddsa-jcs-2022 as a temporary alias
                    // for jcs-eddsa-2022 (no context injection)
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
