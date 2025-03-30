use chrono::{DateTime, Utc};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use apx_core::{
    caip2::{Namespace as ChainNamespace},
    caip10::{AccountId as ChainAccountId},
    crypto_eddsa::Ed25519SecretKey,
    crypto_rsa::rsa_secret_key_to_pkcs1_der,
    did::Did,
};

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
    DatabaseTypeError,
};
use crate::profiles::{
    queries::create_profile,
    types::{
        DbActorProfile,
        MentionPolicy,
        ProfileCreateData,
        WebfingerHostname,
    },
};

use super::types::{
    AccountAdminInfo,
    ClientConfig,
    DbClientConfig,
    DbInviteCode,
    DbPortableUser,
    DbUser,
    PortableUser,
    PortableUserData,
    Role,
    User,
    UserCreateData,
};
use super::utils::generate_invite_code;

pub async fn create_invite_code(
    db_client: &impl DatabaseClient,
    note: Option<&str>,
) -> Result<String, DatabaseError> {
    let invite_code = generate_invite_code();
    db_client.execute(
        "
        INSERT INTO user_invite_code (code, note)
        VALUES ($1, $2)
        ",
        &[&invite_code, &note],
    ).await?;
    Ok(invite_code)
}

pub async fn get_invite_codes(
    db_client: &impl DatabaseClient,
) -> Result<Vec<DbInviteCode>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT user_invite_code
        FROM user_invite_code
        WHERE used = FALSE
        ",
        &[],
    ).await?;
    let codes = rows.iter()
        .map(|row| row.try_get("user_invite_code"))
        .collect::<Result<_, _>>()?;
    Ok(codes)
}

pub async fn is_valid_invite_code(
    db_client: &impl DatabaseClient,
    invite_code: &str,
) -> Result<bool, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1 FROM user_invite_code
        WHERE code = $1 AND used = FALSE
        ",
        &[&invite_code],
    ).await?;
    Ok(maybe_row.is_some())
}

async fn use_invite_code(
    db_client: &impl DatabaseClient,
    invite_code: &str,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE user_invite_code
        SET used = TRUE
        WHERE code = $1 AND used = FALSE
        ",
        &[&invite_code],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("invite code"));
    };
    Ok(())
}

pub async fn check_local_username_unique(
    db_client: &impl DatabaseClient,
    username: &str,
) -> Result<(), DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1
        FROM actor_profile
        WHERE
            (user_id IS NOT NULL OR portable_user_id IS NOT NULL)
            AND actor_profile.username ILIKE $1
        LIMIT 1
        ",
        &[&username],
    ).await?;
    if maybe_row.is_some() {
        return Err(DatabaseError::AlreadyExists("user"));
    };
    Ok(())
}

pub async fn create_user(
    db_client: &mut impl DatabaseClient,
    user_data: UserCreateData,
) -> Result<User, DatabaseError> {
    user_data.check_consistency()?;
    let mut transaction = db_client.transaction().await?;
    // Prevent changes to actor_profile table
    transaction.execute(
        "LOCK TABLE actor_profile IN EXCLUSIVE MODE",
        &[],
    ).await?;
    // Ensure there are no local accounts with a similar name
    check_local_username_unique(&transaction, &user_data.username).await?;
    // Use invite code
    if let Some(ref invite_code) = user_data.invite_code {
        use_invite_code(&transaction, invite_code).await?;
    };
    // Create profile
    let profile_data = ProfileCreateData {
        username: user_data.username.clone(),
        hostname: WebfingerHostname::Local,
        display_name: None,
        bio: None,
        avatar: None,
        banner: None,
        is_automated: false,
        manually_approves_followers: false,
        mention_policy: MentionPolicy::None,
        public_keys: vec![],
        identity_proofs: vec![],
        payment_options: vec![],
        extra_fields: vec![],
        aliases: vec![],
        emojis: vec![],
        actor_json: None,
    };
    let db_profile = create_profile(&mut transaction, profile_data).await?;
    // Create user
    let row = transaction.query_one(
        "
        INSERT INTO user_account (
            id,
            password_digest,
            login_address_ethereum,
            login_address_monero,
            rsa_private_key,
            ed25519_private_key,
            invite_code,
            user_role
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING user_account
        ",
        &[
            &db_profile.id,
            &user_data.password_digest,
            &user_data.login_address_ethereum,
            &user_data.login_address_monero,
            &user_data.rsa_secret_key,
            &user_data.ed25519_secret_key,
            &user_data.invite_code,
            &user_data.role,
        ],
    ).await.map_err(catch_unique_violation("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    // Create reverse FK
    let row = transaction.query_one(
        "
        UPDATE actor_profile
        SET user_id = actor_profile.id
        WHERE id = $1
        RETURNING actor_profile
        ",
        &[&db_profile.id],
    ).await?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    transaction.commit().await?;
    Ok(user)
}

pub async fn set_user_password(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    password_digest: &str,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE user_account SET password_digest = $1
        WHERE id = $2
        ",
        &[&password_digest, &user_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("user"));
    };
    Ok(())
}

pub(super) async fn set_user_ed25519_secret_key(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    secret_key: Ed25519SecretKey,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE user_account SET ed25519_private_key = $1
        WHERE id = $2
        ",
        &[&secret_key.to_vec(), &user_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("user"));
    };
    Ok(())
}

pub async fn set_user_role(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    role: Role,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE user_account SET user_role = $1
        WHERE id = $2
        ",
        &[&role, &user_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("user"));
    };
    Ok(())
}

pub async fn update_client_config(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    client_name: &str,
    client_config_value: &JsonValue,
) -> Result<ClientConfig, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE user_account
        SET client_config = jsonb_set(client_config, ARRAY[$1], $2, true)
        WHERE id = $3
        RETURNING client_config
        ",
        &[&client_name, &client_config_value, &user_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let client_config: DbClientConfig = row.try_get("client_config")?;
    Ok(client_config.into_inner())
}

pub async fn get_user_by_id(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE id = $1
        ",
        &[&user_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn get_user_by_name(
    db_client: &impl DatabaseClient,
    username: &str,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE actor_profile.username = $1
        ",
        &[&username],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn is_registered_user(
    db_client: &impl DatabaseClient,
    username: &str,
) -> Result<bool, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1 FROM user_account JOIN actor_profile USING (id)
        WHERE actor_profile.username = $1
        ",
        &[&username],
    ).await?;
    Ok(maybe_row.is_some())
}

pub async fn get_user_by_login_address(
    db_client: &impl DatabaseClient,
    account_id: &ChainAccountId,
) -> Result<User, DatabaseError> {
    let column_name = match account_id.chain_id.namespace() {
        ChainNamespace::Eip155 => "login_address_ethereum",
        ChainNamespace::Monero => "login_address_monero",
    };
    let statement = format!(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE {column_name} = $1
        ",
        column_name=column_name,
    );
    let maybe_row = db_client.query_opt(
        &statement,
        &[&account_id.address],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn get_user_by_did(
    db_client: &impl DatabaseClient,
    did: &Did,
) -> Result<User, DatabaseError> {
    // DIDs must be locally unique
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE
            EXISTS (
                SELECT 1
                FROM jsonb_array_elements(actor_profile.identity_proofs) AS proof
                WHERE proof ->> 'issuer' = $1
            )
        ",
        &[&did.to_string()],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    Ok(user)
}

#[allow(dead_code)]
async fn get_user_by_identity_key(
    db_client: &impl DatabaseClient,
    identity_key: &str,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE actor_profile.identity_key = $1
        ",
        &[&identity_key],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn get_users_by_role(
    db_client: &impl DatabaseClient,
    role: Role,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile.id
        FROM user_account
        JOIN actor_profile USING (id)
        WHERE user_account.user_role = $1
        ",
        &[&role],
    ).await?;
    let users = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(users)
}

pub async fn get_admin_user(
    db_client: &impl DatabaseClient,
) -> Result<Option<User>, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE user_role = $1
        ORDER BY actor_profile.created_at DESC
        LIMIT 1
        ",
        &[&Role::Admin],
    ).await?;
    let maybe_user = match maybe_row {
        Some(row) => {
            let db_user: DbUser = row.try_get("user_account")?;
            let db_profile: DbActorProfile = row.try_get("actor_profile")?;
            let user = User::new(db_user, db_profile)?;
            Some(user)
        },
        None => None,
    };
    Ok(maybe_user)
}

pub async fn get_user_count(
    db_client: &impl DatabaseClient,
) -> Result<i64, DatabaseError> {
    let row = db_client.query_one(
        "SELECT count(user_account) FROM user_account",
        &[],
    ).await?;
    let count = row.try_get("count")?;
    Ok(count)
}

pub async fn get_active_user_count(
    db_client: &impl DatabaseClient,
    not_before: DateTime<Utc>,
) -> Result<i64, DatabaseError> {
    let row = db_client.query_one(
        "
        SELECT count(DISTINCT user_account)
        FROM user_account
        JOIN oauth_token ON (oauth_token.owner_id = user_account.id)
        WHERE oauth_token.created_at > $1
        ",
        &[&not_before],
    ).await?;
    let count = row.try_get("count")?;
    Ok(count)
}

pub async fn get_accounts_for_admin(
    db_client: &impl DatabaseClient,
) -> Result<Vec<AccountAdminInfo>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT
            actor_profile,
            portable_user_account.id IS NOT NULL as is_portable,
            user_account.user_role AS role,
            max(oauth_token.created_at) AS last_login
        FROM actor_profile
        LEFT JOIN user_account USING (id)
        LEFT JOIN portable_user_account USING (id)
        LEFT JOIN oauth_token ON (oauth_token.owner_id = user_account.id)
        WHERE user_id IS NOT NULL OR portable_user_id IS NOT NULL
        GROUP BY
            actor_profile.id,
            user_account.id,
            portable_user_account.id
        ORDER BY actor_profile.created_at DESC
        ",
        &[],
    ).await?;
    let users = rows.iter()
        .map(AccountAdminInfo::try_from)
        .collect::<Result<_, _>>()?;
    Ok(users)
}

pub async fn create_portable_user(
    db_client: &mut impl DatabaseClient,
    user_data: PortableUserData,
) -> Result<PortableUser, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Use invite code
    use_invite_code(&transaction, &user_data.invite_code).await?;
    // Create user
    let rsa_secret_key_der =
        rsa_secret_key_to_pkcs1_der(&user_data.rsa_secret_key)
            .map_err(|_| DatabaseTypeError)?;
    let row = transaction.query_one(
        "
        INSERT INTO portable_user_account (
            id,
            rsa_secret_key,
            ed25519_secret_key,
            invite_code
        )
        VALUES ($1, $2, $3, $4)
        RETURNING portable_user_account
        ",
        &[
            &user_data.profile_id,
            &rsa_secret_key_der,
            &user_data.ed25519_secret_key,
            &user_data.invite_code,
        ],
    ).await.map_err(catch_unique_violation("portable user"))?;
    let db_user: DbPortableUser = row.try_get("portable_user_account")?;
    // Create reverse FK and generate local 'acct'
    let row = transaction.query_one(
        "
        UPDATE actor_profile
        SET
            portable_user_id = actor_profile.id,
            hostname = NULL,
            acct = actor_profile.username
        WHERE id = $1
        RETURNING actor_profile
        ",
        &[&user_data.profile_id],
    ).await?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = PortableUser::new(db_user, db_profile)?;
    transaction.commit().await?;
    Ok(user)
}

pub async fn get_portable_user_by_name(
    db_client: &impl DatabaseClient,
    username: &str,
) -> Result<PortableUser, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT portable_user_account, actor_profile
        FROM portable_user_account JOIN actor_profile USING (id)
        WHERE actor_profile.username = $1
        ",
        &[&username],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbPortableUser = row.try_get("portable_user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = PortableUser::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn get_portable_user_by_actor_id(
    db_client: &impl DatabaseClient,
    actor_id: &str,
) -> Result<PortableUser, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT portable_user_account, actor_profile
        FROM portable_user_account JOIN actor_profile USING (id)
        WHERE actor_profile.actor_id = $1
        ",
        &[&actor_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbPortableUser = row.try_get("portable_user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = PortableUser::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn get_portable_user_by_inbox_id(
    db_client: &impl DatabaseClient,
    collection_id: &str, // canonical
) -> Result<PortableUser, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT portable_user_account, actor_profile
        FROM portable_user_account JOIN actor_profile USING (id)
        WHERE actor_profile.actor_json ->> 'inbox' = $1
        ",
        &[&collection_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbPortableUser = row.try_get("portable_user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = PortableUser::new(db_user, db_profile)?;
    Ok(user)
}

pub async fn get_portable_user_by_outbox_id(
    db_client: &impl DatabaseClient,
    collection_id: &str, // canonical
) -> Result<PortableUser, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT portable_user_account, actor_profile
        FROM portable_user_account JOIN actor_profile USING (id)
        WHERE actor_profile.actor_json ->> 'outbox' = $1
        ",
        &[&collection_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbPortableUser = row.try_get("portable_user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = PortableUser::new(db_user, db_profile)?;
    Ok(user)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use apx_core::{
        crypto_eddsa::generate_weak_ed25519_key,
        crypto_rsa::generate_weak_rsa_key,
    };
    use crate::{
        database::test_utils::create_test_database,
        profiles::types::{
            DbActor,
            DbActorKey,
            WebfingerHostname,
        },
        users::types::Role,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_invite_code() {
        let db_client = &mut create_test_database().await;
        let code = create_invite_code(db_client, Some("test")).await.unwrap();
        assert_eq!(code.len(), 32);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_user() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "myname".to_string(),
            password_digest: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        assert_eq!(user.profile.username, "myname");
        assert_eq!(user.role, Role::NormalUser);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_user_impersonation_protection() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "myname".to_string(),
            password_digest: Some("test".to_string()),
            ..Default::default()
        };
        create_user(db_client, user_data).await.unwrap();
        let another_user_data = UserCreateData {
            username: "myName".to_string(),
            password_digest: Some("test".to_string()),
            ..Default::default()
        };
        let result = create_user(db_client, another_user_data).await;
        assert!(matches!(result, Err(DatabaseError::AlreadyExists("user"))));
    }

    #[tokio::test]
    #[serial]
    async fn test_set_user_ed25519_secret_key() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_digest: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let secret_key = [9; 32];
        set_user_ed25519_secret_key(
            db_client,
            user.id,
            secret_key,
        ).await.unwrap();
        let user = get_user_by_id(db_client, user.id).await.unwrap();
        assert_eq!(user.ed25519_secret_key, secret_key);
    }

    #[tokio::test]
    #[serial]
    async fn test_set_user_role() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_digest: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        assert_eq!(user.role, Role::NormalUser);
        set_user_role(db_client, user.id, Role::ReadOnlyUser).await.unwrap();
        let user = get_user_by_id(db_client, user.id).await.unwrap();
        assert_eq!(user.role, Role::ReadOnlyUser);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_client_config() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_digest: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        assert_eq!(user.client_config.is_empty(), true);
        let client_name = "test";
        let client_config_value = json!({"a": 1});
        let client_config = update_client_config(
            db_client,
            user.id,
            client_name,
            &client_config_value,
        ).await.unwrap();
        assert_eq!(
            client_config.get(client_name).unwrap(),
            &client_config_value,
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_get_admin_user() {
        let db_client = &mut create_test_database().await;
        let maybe_admin = get_admin_user(db_client).await.unwrap();
        assert_eq!(maybe_admin.is_none(), true);

        let user_data = UserCreateData {
            username: "test".to_string(),
            password_digest: Some("test".to_string()),
            role: Role::Admin,
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let maybe_admin = get_admin_user(db_client).await.unwrap();
        assert_eq!(maybe_admin.unwrap().id, user.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_portable_user() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: WebfingerHostname::Unknown,
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(DbActor {
                id: "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor".to_string(),
                gateways: vec!["https://gateway.example".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        profile.check_consistency().unwrap();
        assert!(matches!(profile.hostname(), WebfingerHostname::Unknown));
        let rsa_secret_key = generate_weak_rsa_key().unwrap();
        let ed25519_secret_key = generate_weak_ed25519_key();
        let invite_code =
            create_invite_code(db_client, Some("test")).await.unwrap();
        let user_data = PortableUserData {
            profile_id: profile.id,
            rsa_secret_key: rsa_secret_key.clone(),
            ed25519_secret_key: ed25519_secret_key,
            invite_code: invite_code,
        };
        let user = create_portable_user(db_client, user_data).await.unwrap();
        assert_eq!(user.id, profile.id);
        assert_eq!(user.rsa_secret_key, rsa_secret_key);
        assert_eq!(user.ed25519_secret_key, ed25519_secret_key);
        assert!(matches!(user.profile.hostname(), WebfingerHostname::Local));
    }
}
