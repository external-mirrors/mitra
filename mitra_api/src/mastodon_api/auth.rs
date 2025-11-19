use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    oauth::queries::get_user_by_oauth_token,
    users::types::User,
};

use super::errors::MastodonError;

pub async fn get_current_session(
    db_client: &impl DatabaseClient,
    token: &str,
) -> Result<(i32, User), MastodonError> {
    let session_info = match get_user_by_oauth_token(db_client, token).await {
        Ok(session_info) => session_info,
        Err(DatabaseError::NotFound(_)) => {
            return Err(MastodonError::AuthError("access token is invalid"));
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(session_info)
}

pub async fn get_current_user(
    db_client: &impl DatabaseClient,
    token: &str,
) -> Result<User, MastodonError> {
    let (_, user) = get_current_session(db_client, token).await?;
    Ok(user)
}
