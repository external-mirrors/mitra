use std::time::Duration;

use actix_governor::{Governor, GovernorExtractor};
use actix_multipart::form::MultipartForm;
use actix_web::{
    dev::ConnectionInfo,
    get,
    http::{
        header::{CacheControl, CacheDirective},
        Uri,
    },
    patch,
    post,
    web,
    Either,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use chrono::Utc;
use uuid::Uuid;

use apx_core::{
    caip2::ChainId,
    crypto_eddsa::generate_ed25519_key,
    crypto_rsa::{
        generate_rsa_key,
        rsa_secret_key_to_pkcs8_pem,
    },
    did::Did,
    did_pkh::DidPkh,
    json_signatures::{
        create::IntegrityProofConfig,
        verify::{
            verify_blake2_ed25519_json_signature,
            verify_eddsa_json_signature,
            verify_eip191_json_signature,
        },
    },
    minisign::{
        minisign_key_to_did,
        parse_minisign_signature_file,
    },
};
use mitra_activitypub::{
    builders::{
        follow::follow_or_create_request,
        reject_follow::prepare_reject_follow,
        undo_follow::prepare_undo_follow,
        update_person::prepare_update_person,
    },
    identifiers::local_actor_id,
    identity::{
        create_identity_claim_fep_c390,
        create_identity_proof_fep_c390,
    },
    queues::FetcherJobData,
};
use mitra_adapters::roles::from_default_role;
use mitra_config::{
    AuthenticationMethod,
    Config,
    RegistrationType,
};
use mitra_models::{
    custom_feeds::queries::get_custom_feeds_by_source,
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    notifications::helpers::create_signup_notifications,
    posts::queries::get_posts_by_author,
    profiles::queries::{
        get_profile_by_acct,
        get_profile_by_id,
        search_profiles_by_did,
        update_profile,
    },
    profiles::types::{
        IdentityProofType,
        ProfileUpdateData,
    },
    relationships::helpers::remove_follower,
    relationships::queries::{
        get_followers_paginated,
        get_following_paginated,
        hide_replies,
        hide_reposts,
        show_replies,
        show_reposts,
        unfollow,
        mute,
        unmute,
    },
    subscriptions::queries::get_incoming_subscriptions,
    users::queries::{
        create_user,
        get_user_by_did,
        is_valid_invite_code,
    },
    users::types::{Permission, UserCreateData},
};
use mitra_services::{
    ethereum::eip4361::verify_eip4361_signature,
    media::MediaStorage,
    monero::caip122::verify_monero_caip122_signature,
};
use mitra_utils::{
    identicons::{generate_identicon, generate_pixel},
    passwords::hash_password,
};
use mitra_validators::{
    errors::ValidationError,
    profiles::{clean_profile_update_data, validate_identity_proofs},
    users::validate_local_username,
};

use crate::http::{
    get_request_base_url,
    ratelimit_config,
    FormOrJson,
    MultiQuery,
};
use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
    lists::types::List,
    pagination::{get_last_item, get_paginated_response},
    search::helpers::search_profiles_only,
    statuses::helpers::get_paginated_status_list,
};

use super::helpers::{
    get_aliases,
    get_relationship,
    get_relationships,
    parse_microsyntaxes,
};
use super::types::{
    Account,
    AccountCreateData,
    AccountUpdateData,
    AccountUpdateMultipartForm,
    ApiSubscription,
    AUTHENTICATION_METHOD_CAIP122_MONERO,
    AUTHENTICATION_METHOD_EIP4361,
    AUTHENTICATION_METHOD_PASSWORD,
    FollowData,
    FollowListQueryParams,
    IdenticonQueryParams,
    IdentityClaim,
    IdentityClaimQueryParams,
    IdentityProofData,
    LookupAcctQueryParams,
    RelationshipQueryParams,
    SearchAcctQueryParams,
    SearchDidQueryParams,
    StatusListQueryParams,
    SubscriptionListQueryParams,
};

pub async fn create_account(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_data: web::Json<AccountCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let instance = config.instance();
    // Validate
    if config.registration.registration_type == RegistrationType::Invite {
        let invite_code = account_data.invite_code.as_ref()
            .ok_or(ValidationError("invite code is required"))?;
        if !is_valid_invite_code(db_client, invite_code).await? {
            return Err(ValidationError("invalid invite code").into());
        };
    };

    validate_local_username(&account_data.username)?;

    let authentication_method = match account_data.authentication_method.as_str() {
        AUTHENTICATION_METHOD_PASSWORD => AuthenticationMethod::Password,
        AUTHENTICATION_METHOD_EIP4361 => AuthenticationMethod::Eip4361,
        AUTHENTICATION_METHOD_CAIP122_MONERO => AuthenticationMethod::Caip122Monero,
        _ => {
            return Err(ValidationError("unsupported authentication method").into());
        },
    };
    if !config.authentication_methods.contains(&authentication_method) {
        return Err(MastodonError::NotSupported);
    };
    let maybe_password_digest = if authentication_method == AuthenticationMethod::Password {
        let password = account_data.password.as_ref()
            .ok_or(ValidationError("password is required"))?;
        let password_digest = hash_password(password)
            .map_err(|_| MastodonError::InternalError)?;
        Some(password_digest)
    } else {
        None
    };
    let maybe_ethereum_address = if authentication_method == AuthenticationMethod::Eip4361 {
        let message = account_data.message.as_ref()
            .ok_or(ValidationError("message is required"))?;
        let signature = account_data.signature.as_ref()
            .ok_or(ValidationError("signature is required"))?;
        let session_data = verify_eip4361_signature(
            message,
            signature,
            &instance.hostname(),
            &config.login_message,
        ).map_err(|err| MastodonError::ValidationError(err.to_string()))?;
        // Don't remember nonce to avoid extra signature requests
        // during registration
        Some(session_data.account_id.address)
    } else {
        None
    };
    let maybe_monero_address = if authentication_method == AuthenticationMethod::Caip122Monero {
        let message = account_data.message.as_ref()
            .ok_or(ValidationError("message is required"))?;
        let signature = account_data.signature.as_ref()
            .ok_or(ValidationError("signature is required"))?;
        let monero_config = config.monero_config()
            .ok_or(MastodonError::NotSupported)?;
        let session_data = verify_monero_caip122_signature(
            monero_config,
            &instance.hostname(),
            &config.login_message,
            message,
            signature,
        ).await.map_err(|_| ValidationError("invalid signature"))?;
        Some(session_data.account_id.address)
    } else {
        None
    };

    // Generate RSA private key for actor
    let rsa_secret_key = match web::block(generate_rsa_key).await {
        Ok(Ok(secret_key)) => secret_key,
        _ => return Err(MastodonError::InternalError),
    };
    let rsa_secret_key_pem = rsa_secret_key_to_pkcs8_pem(&rsa_secret_key)
        .map_err(|_| MastodonError::InternalError)?;
    let ed25519_secret_key = generate_ed25519_key();

    let AccountCreateData { username, invite_code, .. } =
        account_data.into_inner();
    let role = from_default_role(&config.registration.default_role);
    let user_data = UserCreateData {
        username,
        password_digest: maybe_password_digest,
        login_address_ethereum: maybe_ethereum_address,
        login_address_monero: maybe_monero_address,
        rsa_secret_key: rsa_secret_key_pem,
        ed25519_secret_key: ed25519_secret_key,
        invite_code,
        role,
    };
    let user = match create_user(db_client, user_data).await {
        Ok(user) => user,
        Err(DatabaseError::AlreadyExists(_)) =>
            return Err(ValidationError("user already exists").into()),
        Err(other_error) => return Err(other_error.into()),
    };
    create_signup_notifications(db_client, user.id).await?;
    log::warn!("created user {}", user);
    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &instance.url(),
        user,
    );
    Ok(HttpResponse::Created().json(account))
}

#[get("/verify_credentials")]
async fn verify_credentials(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_current_user(db_client, auth.token()).await?;
    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[patch("/update_credentials")]
async fn update_credentials(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_data: Either<
        MultipartForm<AccountUpdateMultipartForm>,
        web::Json<AccountUpdateData>,
    >,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let media_storage = MediaStorage::from(config.as_ref());
    let mut profile_data = match account_data {
        Either::Left(form) => {
            form.into_inner().into_profile_data(
                &current_user.profile,
                &config.limits.media,
                &media_storage,
            )?
        },
        Either::Right(data) => {
            data.into_inner().into_profile_data(
                &current_user.profile,
                &config.limits.media,
                &media_storage,
            )?
        },
    };
    parse_microsyntaxes(db_client, &mut profile_data).await?;
    clean_profile_update_data(&mut profile_data)?;
    let (updated_profile, deletion_queue) = update_profile(
        db_client,
        current_user.id,
        profile_data,
    ).await?;
    current_user.profile = updated_profile;
    // Delete orphaned images after update
    deletion_queue.into_job(db_client).await?;

    // Federate
    prepare_update_person(
        db_client,
        &config.instance(),
        &current_user,
    ).await?.save_and_enqueue(db_client).await?;

    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/identity_proof")]
async fn get_identity_claim(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<IdentityClaimQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let (did, proof_type) = match query_params.proof_type.as_str() {
        "ethereum" => {
            let did_pkh = DidPkh::from_ethereum_address(&query_params.signer);
            (Did::Pkh(did_pkh), IdentityProofType::FepC390JcsEip191Proof)
        },
        "minisign" => {
            let did_key = minisign_key_to_did(&query_params.signer)
                .map_err(|_| ValidationError("invalid key"))?;
            (Did::Key(did_key), IdentityProofType::FepC390JcsBlake2Ed25519Proof)
        },
        "minisign-unhashed" => {
            let did_key = minisign_key_to_did(&query_params.signer)
                .map_err(|_| ValidationError("invalid key"))?;
            (Did::Key(did_key), IdentityProofType::FepC390LegacyJcsEddsaProof)
        },
        _ => return Err(ValidationError("unknown proof type").into()),
    };
    let actor_id = local_actor_id(
        &config.instance_url(),
        &current_user.profile.username,
    );
    let created_at = Utc::now();
    let (_claim, message) = create_identity_claim_fep_c390(
        &actor_id,
        &did,
        &proof_type,
        created_at,
    ).map_err(|_| MastodonError::InternalError)?;
    let response = IdentityClaim { did, claim: message, created_at };
    Ok(HttpResponse::Ok().json(response))
}

#[post("/identity_proof")]
async fn create_identity_proof(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    proof_data: web::Json<IdentityProofData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let proof_type = match proof_data.proof_type.as_str() {
        "ethereum" => IdentityProofType::FepC390JcsEip191Proof,
        "minisign" => IdentityProofType::FepC390JcsBlake2Ed25519Proof,
        "minisign-unhashed" => IdentityProofType::FepC390LegacyJcsEddsaProof,
        _ => return Err(ValidationError("unknown proof type").into()),
    };
    let did = proof_data.did.parse::<Did>()
        .map_err(|_| ValidationError("invalid DID"))?;
    // Reject proof if there's another local user with the same DID.
    // This is needed for matching ethereum subscriptions
    match get_user_by_did(db_client, &did).await {
        Ok(user) => {
            if user.id != current_user.id {
                return Err(ValidationError("DID already associated with another user").into());
            };
        },
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let actor_id = local_actor_id(
        &config.instance_url(),
        &current_user.profile.username,
    );
    let (claim, _message) = create_identity_claim_fep_c390(
        &actor_id,
        &did,
        &proof_type,
        proof_data.created_at,
    ).map_err(|_| MastodonError::InternalError)?;
    let claim_value = serde_json::to_value(&claim)
        .expect("claim should be serializable");

    // Verify proof
    let signature_bin = match proof_type {
        IdentityProofType::LegacyEip191IdentityProof
            | IdentityProofType::LegacyMinisignIdentityProof
            => unimplemented!("expected FEP-c390 compatible proof type"),
        IdentityProofType::FepC390JcsBlake2Ed25519Proof => {
            let did_key = did.as_did_key()
                .ok_or(ValidationError("unexpected DID type"))?;
            let signature = parse_minisign_signature_file(&proof_data.signature)
                .map_err(|_| ValidationError("invalid signature encoding"))?;
            if !signature.is_prehashed {
                return Err(ValidationError("invalid signature type").into());
            };
            verify_blake2_ed25519_json_signature(
                did_key,
                &claim_value,
                &signature.value,
            ).map_err(|_| ValidationError("invalid signature"))?;
            signature.value.to_vec()
        },
        IdentityProofType::FepC390JcsEip191Proof => {
            let did_pkh = did.as_did_pkh()
                .ok_or(ValidationError("unexpected DID type"))?;
            if did_pkh.chain_id() != ChainId::ethereum_mainnet() {
                // DID must point to Ethereum Mainnet because it is a valid
                // identifier on any Ethereum chain
                return Err(ValidationError("unsupported chain ID").into());
            };
            if let Some(address) = current_user.public_ethereum_address() {
                // Do not allow to add more than one address proof
                if did_pkh.address() != address {
                    return Err(ValidationError("DID doesn't match current identity").into());
                };
            };
            let signature_bin = hex::decode(&proof_data.signature)
                .map_err(|_| ValidationError("invalid signature encoding"))?;
            verify_eip191_json_signature(
                did_pkh,
                &claim_value,
                &signature_bin,
            ).map_err(|_| ValidationError("invalid signature"))?;
            signature_bin
        },
        IdentityProofType::FepC390LegacyJcsEddsaProof => {
            let did_key = did.as_did_key()
                .ok_or(ValidationError("unexpected DID type"))?;
            let ed25519_key = did_key.try_ed25519_key()
                .map_err(|_| ValidationError("invalid public key"))?;
            let signature = parse_minisign_signature_file(&proof_data.signature)
                .map_err(|_| ValidationError("invalid signature encoding"))?;
            if signature.is_prehashed {
                return Err(ValidationError("invalid signature type").into());
            };
            let proof_config = IntegrityProofConfig::jcs_eddsa_legacy(
                &did_key.to_string(),
                proof_data.created_at,
            );
            let proof_config_value = serde_json::to_value(proof_config)
                .expect("proof config should be serializable");
            verify_eddsa_json_signature(
                &ed25519_key,
                &claim_value,
                &proof_config_value,
                &signature.value,
            ).map_err(|_| ValidationError("invalid signature"))?;
            signature.value.to_vec()
        },
        IdentityProofType::FepC390EddsaJcsNoCiProof => unimplemented!(),
    };

    let proof = create_identity_proof_fep_c390(
        &actor_id,
        &did,
        &proof_type,
        proof_data.created_at,
        &signature_bin,
    );
    let mut profile_data = ProfileUpdateData::from(&current_user.profile);
    profile_data.add_identity_proof(proof);
    validate_identity_proofs(&profile_data.identity_proofs)?;
    // Only identity proofs are updated, media cleanup is not needed
    let (updated_profile, _) = update_profile(
        db_client,
        current_user.id,
        profile_data,
    ).await?;
    current_user.profile = updated_profile;

    // Federate
    prepare_update_person(
        db_client,
        &config.instance(),
        &current_user,
    ).await?.save_and_enqueue(db_client).await?;

    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/relationships")]
async fn get_relationships_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: MultiQuery<RelationshipQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let relationships = get_relationships(
        db_client,
        current_user.id,
        &query_params.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationships))
}

#[get("/lookup")]
async fn lookup_acct(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<LookupAcctQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_acct(db_client, &query_params.acct).await?;
    let account = Account::from_profile(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        profile,
    );
    Ok(HttpResponse::Ok().json(account))
}

async fn search_by_acct(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<SearchAcctQueryParams>,
    governor_result: GovernorExtractor,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    match auth {
        Some(auth) => {
            get_current_user(db_client, auth.token()).await?;
        },
        None => {
            if query_params.resolve {
                // Webfinger queries from unauthenticated users
                // are rate-limited
                if let Some(wait) = governor_result.0.check()
                    .map_err(|_| MastodonError::InternalError)?
                    .map(Duration::from_millis)
                {
                    return Err(MastodonError::RateLimit(wait));
                };
            };
        },
    };
    let profiles = search_profiles_only(
        &config,
        db_client,
        &query_params.q,
        query_params.resolve,
        query_params.limit.inner(),
        query_params.offset,
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(
            &base_url,
            &instance_url,
            profile,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

#[get("/search_did")]
async fn search_by_did(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<SearchDidQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let did: Did = query_params.did.parse()
        .map_err(|_| ValidationError("invalid DID"))?;
    let profiles = search_profiles_by_did(db_client, &did, false).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(
            &base_url,
            &instance_url,
            profile,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

#[get("/identicon")]
async fn generate_identicon_view(
    query_params: web::Query<IdenticonQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let png = if let Some(ref input) = query_params.input {
        generate_identicon(input)
    } else {
        generate_pixel()
    };
    let response = HttpResponse::Ok()
        .content_type("image/png")
        .append_header(CacheControl(vec![CacheDirective::MaxAge(86400 * 3)]))
        .body(png);
    Ok(response)
}

#[get("/{account_id}")]
async fn get_account(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_id(db_client, *account_id).await?;
    let account = Account::from_profile(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        profile,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[post("/{account_id}/follow")]
async fn follow_account(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
    follow_data: Option<FormOrJson<FollowData>>,
) -> Result<HttpResponse, MastodonError> {
    // Some clients may send an empty body
    let follow_data = follow_data
        .map(|data| data.into_inner())
        .unwrap_or_default();
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, *account_id).await?;
    if target.id == current_user.id {
        return Err(ValidationError("target is current user").into());
    };

    follow_or_create_request(
        db_client,
        &config.instance(),
        &current_user,
        &target,
    ).await?;
    if follow_data.reblogs {
        show_reposts(db_client, current_user.id, target.id).await?;
    } else {
        hide_reposts(db_client, current_user.id, target.id).await?;
    };
    if follow_data.replies {
        show_replies(db_client, current_user.id, target.id).await?;
    } else {
        hide_replies(db_client, current_user.id, target.id).await?;
    };
    let relationship = get_relationship(
        db_client,
        current_user.id,
        target.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/unfollow")]
async fn unfollow_account(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, *account_id).await?;
    match unfollow(db_client, current_user.id, target.id).await {
        Ok(maybe_follow_request_deleted) => {
            if let Some(remote_actor) = target.actor_json {
                // Remote follow
                let (
                    follow_request_id,
                    follow_request_has_deprecated_ap_id,
                ) = maybe_follow_request_deleted
                    .ok_or(MastodonError::InternalError)?;
                prepare_undo_follow(
                    &config.instance(),
                    &current_user,
                    &remote_actor,
                    follow_request_id,
                    follow_request_has_deprecated_ap_id,
                )?.save_and_enqueue(db_client).await?;
            };
        },
        Err(DatabaseError::NotFound(_)) => (), // not following
        Err(other_error) => return Err(other_error.into()),
    };

    let relationship = get_relationship(
        db_client,
        current_user.id,
        target.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/remove_from_followers")]
async fn remove_follower_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let follower = get_profile_by_id(db_client, *account_id).await?;
    let maybe_follow_activity_id = match remove_follower(
        db_client,
        follower.id,
        current_user.id,
    ).await {
        Ok(maybe_follow_activity_id) => maybe_follow_activity_id,
        // Not a follower, don't send Reject(Follow) activity
        Err(DatabaseError::NotFound(_)) => None,
        Err(other_error) => return Err(other_error.into()),
    };
    if let Some(follow_activity_id) = maybe_follow_activity_id {
        let remote_actor = follower.actor_json
            .expect("actor data should be present");
        prepare_reject_follow(
            &config.instance(),
            &current_user,
            &remote_actor,
            &follow_activity_id,
        )?.save_and_enqueue(db_client).await?;
    };
    let relationship = get_relationship(
        db_client,
        current_user.id,
        follower.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/mute")]
async fn mute_account(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, *account_id).await?;
    if target.id == current_user.id {
        return Err(ValidationError("target is current user").into());
    };

    match mute(db_client, current_user.id, target.id).await {
        Ok(_) => (),
        Err(DatabaseError::AlreadyExists(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };

    let relationship = get_relationship(
        db_client,
        current_user.id,
        target.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/unmute")]
async fn unmute_account(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, *account_id).await?;

    match unmute(db_client, current_user.id, target.id).await {
        Ok(_) => (),
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };

    let relationship = get_relationship(
        db_client,
        current_user.id,
        target.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[get("/{account_id}/statuses")]
async fn get_account_statuses(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    account_id: web::Path<Uuid>,
    query_params: web::Query<StatusListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let profile = get_profile_by_id(db_client, *account_id).await?;
    // Include reposts but not replies
    let posts = get_posts_by_author(
        db_client,
        profile.id,
        maybe_current_user.as_ref().map(|user| user.id),
        !query_params.exclude_replies,
        !query_params.exclude_reblogs,
        query_params.pinned,
        query_params.only_media,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let response = get_paginated_status_list(
        db_client,
        &base_url,
        &instance_url,
        &request_uri,
        maybe_current_user.as_ref(),
        posts,
        &query_params.limit,
    ).await?;
    Ok(response)
}

#[get("/{account_id}/followers")]
async fn get_account_followers(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, *account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let accounts: Vec<Account> = vec![];
        return Ok(HttpResponse::Ok().json(accounts));
    };
    let followers = get_followers_paginated(
        db_client,
        profile.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let maybe_last_id = get_last_item(&followers, &query_params.limit)
        .map(|item| item.related_id);
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = followers.into_iter()
        .map(|item| Account::from_profile(
            &base_url,
            &instance_url,
            item.profile,
        ))
        .collect();
    let response = get_paginated_response(
        &base_url,
        &request_uri,
        accounts,
        maybe_last_id,
    );
    Ok(response)
}

#[get("/{account_id}/following")]
async fn get_account_following(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_uri: Uri,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, *account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let accounts: Vec<Account> = vec![];
        return Ok(HttpResponse::Ok().json(accounts));
    };
    let following = get_following_paginated(
        db_client,
        profile.id,
        query_params.max_id,
        query_params.limit.inner(),
    ).await?;
    let maybe_last_id = get_last_item(&following, &query_params.limit)
        .map(|item| item.related_id);
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = following.into_iter()
        .map(|item| Account::from_profile(
            &base_url,
            &instance_url,
            item.profile,
        ))
        .collect();
    let response = get_paginated_response(
        &base_url,
        &request_uri,
        accounts,
        maybe_last_id,
    );
    Ok(response)
}

#[get("/{account_id}/subscribers")]
async fn get_account_subscribers(
    auth: BearerAuth,
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<SubscriptionListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, *account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let subscriptions: Vec<ApiSubscription> = vec![];
        return Ok(HttpResponse::Ok().json(subscriptions));
    };
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance_url();
    let subscriptions: Vec<ApiSubscription> = get_incoming_subscriptions(
        db_client,
        profile.id,
        query_params.include_expired,
        query_params.max_id,
        query_params.limit.inner(),
    )
        .await?
        .into_iter()
        .map(|subscription| ApiSubscription::from_subscription(
            &base_url,
            &instance_url,
            subscription,
        ))
        .collect();
    Ok(HttpResponse::Ok().json(subscriptions))
}

/// https://docs.joinmastodon.org/methods/accounts/#lists
#[get("/{account_id}/lists")]
async fn get_account_lists(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let feeds = get_custom_feeds_by_source(
        db_client,
        current_user.id,
        *account_id,
    ).await?;
    let lists: Vec<_> = feeds.into_iter().map(List::from_db).collect();
    Ok(HttpResponse::Ok().json(lists))
}

#[get("/{account_id}/aliases/all")]
async fn get_account_aliases(
    config: web::Data<Config>,
    connection_info: ConnectionInfo,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_id(db_client, *account_id).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance_url();
    let aliases = get_aliases(
        db_client,
        &base_url,
        &instance_url,
        &profile,
    ).await?;
    Ok(HttpResponse::Ok().json(aliases))
}

#[post("/{account_id}/load_activities")]
async fn load_activities(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::DeleteAnyProfile) {
        return Err(MastodonError::PermissionError);
    };
    let profile = get_profile_by_id(db_client, *account_id).await?;
    let job_data = if let Some(ref remote_actor) = profile.actor_json {
        FetcherJobData::Outbox { actor_id: remote_actor.id.clone() }
    } else {
        // Local profile
        return Err(MastodonError::NotFoundError("profile"));
    };
    job_data.into_job(db_client).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn account_api_scope() -> Scope {
    // Two requests per 30 seconds; to be used with extractor
    let search_limit = ratelimit_config(2, 30, true);
    // TODO: use Resource::get() (requires actix-web 4.4.0)
    let search_by_acct_limited = web::resource("/search").route(
        web::get()
            .to(search_by_acct)
            .wrap(Governor::new(&search_limit)));
    let registration_limit = ratelimit_config(2, 300, false);
    let create_account_limited = web::resource("").route(
        web::post()
            .to(create_account)
            .wrap(Governor::new(&registration_limit)));
    web::scope("/v1/accounts")
        // Routes without account ID
        .service(create_account_limited)
        .service(verify_credentials)
        .service(update_credentials)
        .service(get_identity_claim)
        .service(create_identity_proof)
        .service(get_relationships_view)
        .service(lookup_acct)
        .service(search_by_acct_limited)
        .service(search_by_did)
        .service(generate_identicon_view)
        // Routes with account ID
        .service(get_account)
        .service(follow_account)
        .service(unfollow_account)
        .service(remove_follower_view)
        .service(mute_account)
        .service(unmute_account)
        .service(get_account_statuses)
        .service(get_account_followers)
        .service(get_account_following)
        .service(get_account_subscribers)
        .service(get_account_lists)
        .service(get_account_aliases)
        .service(load_activities)
}
