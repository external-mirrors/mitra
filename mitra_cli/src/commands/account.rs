use anyhow::Error;
use apx_sdk::core::{
    crypto::{
        eddsa::generate_ed25519_key,
        rsa::{
            generate_rsa_key,
            rsa_secret_key_to_pkcs8_pem,
        },
    },
};
use clap::Parser;

use mitra_adapters::{
    roles::{
        from_default_role,
        role_from_str,
        role_to_str,
        ALLOWED_ROLES,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    oauth::queries::delete_oauth_tokens,
    profiles::types::ANONYMOUS,
    users::{
        helpers::get_user_by_id_or_name,
        queries::{
            create_automated_account,
            create_user,
            get_accounts_for_admin,
            set_user_password,
            set_user_role,
        },
        types::{
            AutomatedAccountData,
            AutomatedAccountType,
            UserCreateData,
        },
    },
};
use mitra_utils::passwords::hash_password;
use mitra_validators::users::validate_local_username;

/// Create new account
#[derive(Parser)]
#[command(visible_alias = "create-user")]
pub struct CreateAccount {
    username: String,
    password: String,
    #[arg(value_parser = ALLOWED_ROLES)]
    role: Option<String>,
}

impl CreateAccount {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        validate_local_username(&self.username)?;
        let password_digest = hash_password(&self.password)?;
        let rsa_secret_key = generate_rsa_key()?;
        let rsa_secret_key_pem =
            rsa_secret_key_to_pkcs8_pem(&rsa_secret_key)?;
        let ed25519_secret_key = generate_ed25519_key();
        let role = match &self.role {
            Some(value) => role_from_str(value)?,
            None => from_default_role(&config.registration.default_role),
        };
        let user_data = UserCreateData {
            username: self.username,
            password_digest: Some(password_digest),
            login_address_ethereum: None,
            login_address_monero: None,
            rsa_secret_key: rsa_secret_key_pem,
            ed25519_secret_key: ed25519_secret_key,
            invite_code: None,
            role,
        };
        create_user(db_client, user_data).await?;
        println!("account created");
        Ok(())
    }
}

/// Create system account
#[derive(Parser)]
#[clap(hide = true)]
pub struct CreateSystemAccount;

impl CreateSystemAccount {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let instance = config.instance();
        let account_data = AutomatedAccountData {
            username: ANONYMOUS.to_owned(),
            account_type: AutomatedAccountType::Anonymous,
            rsa_secret_key: instance.rsa_secret_key,
            ed25519_secret_key: instance.ed25519_secret_key,
        };
        create_automated_account(db_client, account_data).await?;
        println!("account created");
        Ok(())
    }
}

/// List local users
#[derive(Parser)]
#[command(visible_alias = "list-users")]
pub struct ListAccounts;

impl ListAccounts {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let accounts = get_accounts_for_admin(db_client).await?;
        println!(
            "{0: <40} | {1: <35} | {2: <20} | {3: <35} | {4: <35}",
            "ID", "username", "role", "created", "last login",
        );
        for account in accounts {
            let role = match account.role {
                Some(role) => role_to_str(role),
                None => "user (portable)",
            };
            println!(
                "{0: <40} | {1: <35} | {2: <20} | {3: <35} | {4: <35}",
                account.profile.id.to_string(),
                account.profile.username,
                role,
                account.profile.created_at.to_string(),
                account.last_login.map(|dt| dt.to_string()).unwrap_or_default(),
            );
        };
        Ok(())
    }
}

/// Set password
#[derive(Parser)]
pub struct SetPassword {
    id_or_name: String,
    password: String,
}

impl SetPassword {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let user = get_user_by_id_or_name(
            db_client,
            &self.id_or_name,
        ).await?;
        let password_digest = hash_password(&self.password)?;
        set_user_password(db_client, user.id, &password_digest).await?;
        // Revoke all sessions
        delete_oauth_tokens(db_client, user.id).await?;
        println!("password updated");
        Ok(())
    }
}

/// Change user's role
#[derive(Parser)]
pub struct SetRole {
    id_or_name: String,
    #[arg(value_parser = ALLOWED_ROLES)]
    role: String,
}

impl SetRole {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let user = get_user_by_id_or_name(
            db_client,
            &self.id_or_name,
        ).await?;
        let role = role_from_str(&self.role)?;
        set_user_role(db_client, user.id, role).await?;
        println!("role changed");
        Ok(())
    }
}

/// Revoke user's OAuth access tokens
#[derive(Parser)]
pub struct RevokeOauthTokens {
    id_or_name: String,
}

impl RevokeOauthTokens {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let user = get_user_by_id_or_name(
            db_client,
            &self.id_or_name,
        ).await?;
        delete_oauth_tokens(db_client, user.id).await?;
        println!("access tokens revoked");
        Ok(())
    }
}
