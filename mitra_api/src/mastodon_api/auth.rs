use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    oauth::queries::get_user_by_oauth_token,
    users::types::User,
};

use super::errors::MastodonError;

pub async fn get_current_user(
    db_client: &impl DatabaseClient,
    token: &str,
) -> Result<User, MastodonError> {
    let user = match get_user_by_oauth_token(db_client, token).await {
        Ok(user) => user,
        Err(DatabaseError::NotFound(_)) => {
            return Err(MastodonError::AuthError("access token is invalid"));
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(user)
}
