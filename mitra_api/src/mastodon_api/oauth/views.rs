use actix_governor::Governor;
use actix_multipart::form::MultipartForm;
use actix_web::{
    body::{BoxBody, EitherBody},
    dev::{ServiceFactory, ServiceRequest, ServiceResponse},
    get,
    http::header as http_header,
    middleware::{ErrorHandlers, ErrorHandlerResponse},
    post,
    web,
    Either,
    Error as ActixError,
    HttpResponse,
    Scope as ActixScope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use chrono::{TimeDelta, Utc};
use log::Level;

use mitra_config::Config;
use mitra_models::{
    caip122::queries::is_valid_caip122_nonce,
    database::{
        get_database_client,
        DatabaseConnectionPool,
        DatabaseError,
    },
    oauth::queries::{
        create_oauth_authorization,
        delete_oauth_token,
        get_oauth_app_by_client_id,
        get_user_by_authorization_code,
        save_oauth_token,
    },
    users::queries::{
        get_user_by_name,
        get_user_by_login_address,
    },
};
use mitra_services::{
    ethereum::eip4361::verify_eip4361_signature,
    monero::caip122::verify_monero_caip122_signature,
};
use mitra_utils::passwords::verify_password;
use mitra_validators::errors::ValidationError;

use crate::{
    http::{
        log_response_error,
        ratelimit_config,
        ContentSecurityPolicy,
        JsonOrForm,
    },
    mastodon_api::{
        auth::get_current_user,
        errors::MastodonError,
    },
};

use super::types::{
    AuthorizationRequest,
    AuthorizationQueryParams,
    RevocationRequest,
    TokenRequest,
    TokenRequestMultipartForm,
    TokenResponse,
};
use super::utils::{
    generate_oauth_token,
    render_authorization_page,
    render_authorization_code_page,
    AUTHORIZATION_CODE_LIFETIME,
};

#[get("/authorize")]
async fn authorization_page_view() -> HttpResponse {
    let (page, nonce) = render_authorization_page();
    let mut csp = ContentSecurityPolicy::default();
    csp.insert("style-src", &format!("'self' 'nonce-{nonce}'"));
    HttpResponse::Ok()
        .content_type("text/html")
        .append_header((http_header::CONTENT_SECURITY_POLICY, csp.into_string()))
        .body(page)
}

#[post("/authorize")]
async fn authorize_view(
    db_pool: web::Data<DatabaseConnectionPool>,
    form_data: web::Form<AuthorizationRequest>,
    query_params: web::Query<AuthorizationQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &form_data.username).await?;
    let password_digest = user.password_digest.as_ref()
        .ok_or(ValidationError("password auth is disabled"))?;
    let password_correct = verify_password(
        password_digest,
        &form_data.password,
    ).map_err(MastodonError::from_internal)?;
    if !password_correct {
        return Err(ValidationError("incorrect password").into());
    };
    if query_params.response_type != "code" {
        return Err(ValidationError("invalid response type").into());
    };
    let oauth_app = get_oauth_app_by_client_id(
        db_client,
        query_params.client_id,
    ).await?;
    if oauth_app.redirect_uri != query_params.redirect_uri {
        return Err(ValidationError("invalid redirect_uri parameter").into());
    };

    let authorization_code = generate_oauth_token();
    let created_at = Utc::now();
    let expires_at = created_at + TimeDelta::seconds(AUTHORIZATION_CODE_LIFETIME);
    create_oauth_authorization(
        db_client,
        &authorization_code,
        user.id,
        oauth_app.id,
        &query_params.scope.replace('+', " "),
        created_at,
        expires_at,
    ).await?;

    let response = if oauth_app.redirect_uri == "urn:ietf:wg:oauth:2.0:oob" {
        let (page, nonce) = render_authorization_code_page(authorization_code);
        let mut csp = ContentSecurityPolicy::default();
        csp.insert("style-src", &format!("'self' 'nonce-{nonce}'"));
        HttpResponse::Ok()
            .content_type("text/html")
            .append_header((http_header::CONTENT_SECURITY_POLICY, csp.into_string()))
            .body(page)
    } else {
        // https://datatracker.ietf.org/doc/html/rfc6749#section-4.1.2
        let mut redirect_uri = format!(
            "{}?code={}",
            oauth_app.redirect_uri,
            authorization_code,
        );
        if let Some(ref state) = query_params.state {
            redirect_uri += &format!("&state={}", state);
        };
        HttpResponse::Found()
            .append_header((http_header::LOCATION, redirect_uri))
            .finish()
    };
    Ok(response)
}

// https://docs.joinmastodon.org/methods/oauth/#token
async fn token_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: Either<
        JsonOrForm<TokenRequest>,
        MultipartForm<TokenRequestMultipartForm>,
    >,
) -> Result<HttpResponse, MastodonError> {
    let request_data = match request_data {
        Either::Left(data) => data.into_inner(),
        Either::Right(form) => form.into_inner().into(),
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_oauth_app = if let Some(client_id) = request_data.client_id {
        let oauth_app = match get_oauth_app_by_client_id(db_client, client_id).await {
            Ok(app) => app,
            Err(DatabaseError::NotFound(_)) =>
                return Err(MastodonError::AuthError("invalid client credentials")),
            Err(other_error) => return Err(other_error.into()),
        };
        if let Some(client_secret) = request_data.client_secret {
            if client_secret != oauth_app.client_secret {
                log::warn!("incorrect client secret");
            };
        } else {
            log::warn!("client secret is not provided");
        };
        Some(oauth_app)
    } else {
        None
    };
    let user = match request_data.grant_type.as_str() {
        "authorization_code" => {
            // https://www.rfc-editor.org/rfc/rfc6749#section-4.1.3
            let authorization_code = request_data.code.as_ref()
                .ok_or(ValidationError("authorization code is required"))?;
            let client_id = request_data.client_id
                .ok_or(ValidationError("client ID is required"))?;
            get_user_by_authorization_code(
                db_client,
                client_id,
                authorization_code,
            ).await?
        },
        "password" => {
            // OAuth 2.0 Password Grant
            // https://oauth.net/2/grant-types/password/
            let username = request_data.username.as_ref()
                .ok_or(ValidationError("username is required"))?;
            let user = get_user_by_name(db_client, username).await?;
            let password = request_data.password.as_ref()
                .ok_or(ValidationError("password is required"))?;
            let password_digest = user.password_digest.as_ref()
                .ok_or(ValidationError("password auth is disabled"))?;
            let password_correct = verify_password(
                password_digest,
                password,
            ).map_err(MastodonError::from_internal)?;
            if !password_correct {
                return Err(ValidationError("incorrect password").into());
            };
            user
        },
        "eip4361" => {
            let message = request_data.message.as_ref()
                .ok_or(ValidationError("message is required"))?;
            let signature = request_data.signature.as_ref()
                .ok_or(ValidationError("signature is required"))?;
            let session_data = verify_eip4361_signature(
                message,
                signature,
                &config.instance().hostname(),
                &config.login_message,
            ).map_err(|err| MastodonError::ValidationError(err.to_string()))?;
            if !is_valid_caip122_nonce(
                db_client,
                &session_data.account_id,
                &session_data.nonce,
            ).await? {
                return Err(ValidationError("nonce can't be reused").into());
            };
            get_user_by_login_address(
                db_client,
                &session_data.account_id,
            ).await?
        },
        "caip122_monero" => {
            let message = request_data.message.as_ref()
                .ok_or(ValidationError("message is required"))?;
            let signature = request_data.signature.as_ref()
                .ok_or(ValidationError("signature is required"))?;
            let monero_config = config.monero_config()
                .ok_or(MastodonError::NotSupported)?;
            let session_data = verify_monero_caip122_signature(
                monero_config,
                &config.instance().hostname(),
                &config.login_message,
                message,
                signature,
            ).await.map_err(|_| ValidationError("invalid signature"))?;
            if !is_valid_caip122_nonce(
                db_client,
                &session_data.account_id,
                &session_data.nonce,
            ).await? {
                return Err(ValidationError("nonce can't be reused").into());
            };
            get_user_by_login_address(
                db_client,
                &session_data.account_id,
            ).await?
        },
        _ => {
            return Err(ValidationError("unsupported grant type").into());
        },
    };
    let access_token = generate_oauth_token();
    let created_at = Utc::now();
    let expires_in = config.authentication_token_lifetime;
    let expires_at = created_at + TimeDelta::seconds(expires_in.into());
    save_oauth_token(
        db_client,
        user.id,
        maybe_oauth_app.as_ref().map(|app| app.id),
        &access_token,
        created_at,
        expires_at,
    ).await?;
    log::warn!(
        "created auth token for user {} (client: {:?})",
        user,
        maybe_oauth_app.map(|app| app.app_name),
    );
    let token_data = TokenResponse::new(
        access_token,
        created_at.timestamp(),
        expires_in,
    );
    let response = HttpResponse::Ok()
        // Required by RFC-6749
        .append_header((http_header::CACHE_CONTROL, "no-store"))
        .json(token_data);
    Ok(response)
}

#[post("/revoke")]
async fn revoke_token_view(
    auth: BearerAuth,
    db_pool: web::Data<DatabaseConnectionPool>,
    request_data: JsonOrForm<RevocationRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    match delete_oauth_token(
        db_client,
        current_user.id,
        &request_data.into_inner().token,
    ).await {
        Ok(_) => (),
        Err(DatabaseError::NotFound(_)) => return Err(MastodonError::PermissionError),
        Err(other_error) => return Err(other_error.into()),
    };
    let empty = serde_json::json!({});
    Ok(HttpResponse::Ok().json(empty))
}

pub fn oauth_api_scope() -> ActixScope<impl ServiceFactory<
    ServiceRequest,
    Config = (),
    Response = ServiceResponse<EitherBody<BoxBody>>,
    Error = ActixError,
    InitError = (),
>> {
    let token_limit = ratelimit_config(5, 120, false);
    let token_view_limited = web::resource("/token").route(
        web::post()
            .to(token_view)
            .wrap(Governor::new(&token_limit)));
    web::scope("/oauth")
        .wrap(ErrorHandlers::new()
            .default_handler_client(|response| {
                log_response_error(Level::Warn, &response);
                Ok(ErrorHandlerResponse::Response(response.map_into_left_body()))
            })
        )
        .service(authorization_page_view)
        .service(authorize_view)
        .service(token_view_limited)
        .service(revoke_token_view)
}
