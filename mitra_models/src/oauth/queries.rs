use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};
use crate::profiles::types::DbActorProfile;
use crate::users::types::{DbUser, User};

use super::{
    types::{OauthApp, OauthAppData, OauthToken},
    utils::hash_oauth_token,
};

pub async fn create_oauth_app(
    db_client: &impl DatabaseClient,
    app_data: OauthAppData,
) -> Result<OauthApp, DatabaseError> {
    let row = db_client.query_one(
        "
        INSERT INTO oauth_application (
            app_name,
            website,
            scopes,
            redirect_uri,
            client_id,
            client_secret
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING oauth_application
        ",
        &[
            &app_data.app_name,
            &app_data.website,
            &app_data.scopes,
            &app_data.redirect_uri,
            &app_data.client_id,
            &app_data.client_secret,
        ],
    ).await.map_err(catch_unique_violation("oauth_application"))?;
    let app = row.try_get("oauth_application")?;
    Ok(app)
}

pub async fn get_oauth_app_by_client_id(
    db_client: &impl DatabaseClient,
    client_id: Uuid,
) -> Result<OauthApp, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT oauth_application
        FROM oauth_application
        WHERE client_id = $1
        ",
        &[&client_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("oauth application"))?;
    let app = row.try_get("oauth_application")?;
    Ok(app)
}

pub async fn create_oauth_authorization(
    db_client: &impl DatabaseClient,
    authorization_code: &str,
    user_id: Uuid,
    application_id: i32,
    scopes: &str,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO oauth_authorization (
            code,
            user_id,
            application_id,
            scopes,
            created_at,
            expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
        &[
            &authorization_code,
            &user_id,
            &application_id,
            &scopes,
            &created_at,
            &expires_at,
        ],
    ).await?;
    Ok(())
}

pub async fn get_user_by_authorization_code(
    db_client: &impl DatabaseClient,
    client_id: Uuid,
    authorization_code: &str,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM oauth_authorization
        JOIN oauth_application
            ON oauth_authorization.application_id = oauth_application.id
        JOIN user_account ON oauth_authorization.user_id = user_account.id
        JOIN actor_profile ON user_account.id = actor_profile.id
        WHERE
            oauth_application.client_id = $1
            AND oauth_authorization.code = $2
            AND oauth_authorization.expires_at > CURRENT_TIMESTAMP
        ",
        &[
            &client_id,
            &authorization_code,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("authorization"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn save_oauth_token(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
    maybe_app_id: Option<i32>,
    token: &str,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<i32, DatabaseError> {
    let token_digest = hash_oauth_token(token);
    let row = db_client.query_one(
        "
        INSERT INTO oauth_token (
            owner_id,
            application_id,
            token_digest,
            created_at,
            expires_at
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING oauth_token.id
        ",
        &[
            &owner_id,
            &maybe_app_id,
            &token_digest,
            &created_at,
            &expires_at,
        ],
    ).await?;
    let token_id = row.try_get("id")?;
    Ok(token_id)
}

pub async fn delete_oauth_token(
    db_client: &mut impl DatabaseClient,
    current_user_id: Uuid,
    token: &str,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let token_digest = hash_oauth_token(token);
    let maybe_row = transaction.query_opt(
        "
        SELECT owner_id FROM oauth_token
        WHERE token_digest = $1
        FOR UPDATE
        ",
        &[&token_digest],
    ).await?;
    if let Some(row) = maybe_row {
        let owner_id: Uuid = row.try_get("owner_id")?;
        if owner_id != current_user_id {
            // Return error if token is owned by a different user
            return Err(DatabaseError::NotFound("token"));
        } else {
            transaction.execute(
                "
                DELETE FROM oauth_token
                WHERE token_digest = $1
                ",
                &[&token_digest],
            ).await?;
        };
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn delete_oauth_token_by_id(
    db_client: &impl DatabaseClient,
    current_user_id: Uuid,
    token_id: i32,
) -> Result<(), DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM oauth_token
        WHERE id = $1 AND owner_id = $2
        ",
        &[&token_id, &current_user_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("oauth token"));
    };
    Ok(())
}

pub async fn delete_oauth_tokens(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "DELETE FROM oauth_token WHERE owner_id = $1",
        &[&owner_id],
    ).await?;
    Ok(())
}

pub async fn get_oauth_tokens(
    db_client: &impl DatabaseClient,
    owner_id: Uuid,
) -> Result<Vec<OauthToken>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT
            oauth_token.id,
            oauth_token.created_at,
            oauth_token.expires_at,
            oauth_application.app_name
        FROM oauth_token
        LEFT JOIN oauth_application
            ON oauth_token.application_id = oauth_application.id
        WHERE owner_id = $1
        ",
        &[&owner_id],
    ).await?;
    let tokens = rows.into_iter()
        .map(OauthToken::try_from)
        .collect::<Result<_, _>>()?;
    Ok(tokens)
}

pub async fn get_user_by_oauth_token(
    db_client: &impl DatabaseClient,
    token: &str,
) -> Result<(i32, User), DatabaseError> {
    let token_digest = hash_oauth_token(token);
    let maybe_row = db_client.query_opt(
        "
        SELECT oauth_token.id, user_account, actor_profile
        FROM oauth_token
        JOIN user_account ON oauth_token.owner_id = user_account.id
        JOIN actor_profile ON user_account.id = actor_profile.id
        WHERE
            oauth_token.token_digest = $1
            AND oauth_token.expires_at > CURRENT_TIMESTAMP
        ",
        &[&token_digest],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let token_id = row.try_get("id")?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    Ok((token_id, user))
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_oauth_app() {
        let db_client = &create_test_database().await;
        let db_app_data = OauthAppData {
            app_name: "My App".to_string(),
            ..Default::default()
        };
        let app = create_oauth_app(db_client, db_app_data).await.unwrap();
        assert_eq!(app.app_name, "My App");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_oauth_authorization() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "test").await;
        let app_data = OauthAppData {
            app_name: "My App".to_string(),
            ..Default::default()
        };
        let app = create_oauth_app(db_client, app_data).await.unwrap();
        create_oauth_authorization(
            db_client,
            "code",
            user.id,
            app.id,
            "read write",
            Utc::now(),
            Utc::now() + TimeDelta::days(7),
        ).await.unwrap();
        let user_found = get_user_by_authorization_code(
            db_client,
            app.client_id,
            "code",
        ).await.unwrap();
        assert_eq!(user_found.id, user.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_and_delete_oauth_token() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "test").await;
        let app_name = "test_app";
        let app_data = OauthAppData {
            app_name: app_name.to_owned(),
            ..Default::default()
        };
        let app = create_oauth_app(db_client, app_data).await.unwrap();
        let token = "test-token";
        save_oauth_token(
            db_client,
            user.id,
            Some(app.id),
            token,
            Utc::now(),
            Utc::now() + TimeDelta::days(7),
        ).await.unwrap();
        let (token_id, authenticated_user) = get_user_by_oauth_token(
            db_client,
            token,
        ).await.unwrap();
        assert_eq!(authenticated_user.id, user.id);
        let tokens = get_oauth_tokens(db_client, user.id).await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].id, token_id);
        assert_eq!(tokens[0].client_name.as_ref().unwrap(), app_name);

        delete_oauth_token(
            db_client,
            user.id,
            token,
        ).await.unwrap();
        let error = get_user_by_oauth_token(
            db_client,
            token,
        ).await.err().unwrap();
        assert_eq!(error.to_string(), "user not found");
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_oauth_token_by_id() {
        let db_client = &mut create_test_database().await;
        let user = create_test_user(db_client, "test").await;
        let app_name = "test_app";
        let app_data = OauthAppData {
            app_name: app_name.to_owned(),
            ..Default::default()
        };
        let app = create_oauth_app(db_client, app_data).await.unwrap();
        let token = "test-token";
        let token_id = save_oauth_token(
            db_client,
            user.id,
            Some(app.id),
            token,
            Utc::now(),
            Utc::now() + TimeDelta::days(7),
        ).await.unwrap();
        delete_oauth_token_by_id(
            db_client,
            user.id,
            token_id,
        ).await.unwrap();
        let tokens = get_oauth_tokens(db_client, user.id).await.unwrap();
        assert_eq!(tokens.len(), 0);
    }
}
