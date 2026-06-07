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
use clap::{
    Parser,
    Subcommand,
};

use mitra_activitypub::adapters::users::create_or_update_local_actor;
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
    accounts::{
        helpers::get_user_by_id_or_name,
        queries::{
            create_automated_account,
            create_invite_code,
            create_user,
            get_accounts_for_admin,
            get_invite_codes,
            set_user_password,
            set_user_role,
        },
        types::{
            AutomatedAccountData,
            AutomatedAccountType,
            UserCreateData,
        },
    },
    database::{get_database_client, DatabaseConnectionPool},
    oauth::queries::delete_oauth_tokens,
    profiles::types::ANONYMOUS,
};
use mitra_utils::passwords::hash_password;
use mitra_validators::accounts::validate_local_username;

/// Create new account
#[derive(Parser)]
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
        let account = create_user(db_client, user_data).await?;
        create_or_update_local_actor(config, db_client, &account).await?;
        println!("account created");
        Ok(())
    }
}

/// Create system account
#[derive(Parser)]
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

/// Generate invite code
#[derive(Parser)]
pub struct GenerateInviteCode {
    note: Option<String>,
}

impl GenerateInviteCode {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let invite_code = create_invite_code(
            db_client,
            self.note.as_deref(),
        ).await?;
        println!("generated invite code: {}", invite_code);
        Ok(())
    }
}

/// List invite codes
#[derive(Parser)]
pub struct ListInviteCodes;

impl ListInviteCodes {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let invite_codes = get_invite_codes(db_client).await?;
        if invite_codes.is_empty() {
            println!("no invite codes found");
            return Ok(());
        };
        for invite_code in invite_codes {
            if let Some(note) = invite_code.note {
                println!("{} ({})", invite_code.code, note);
            } else {
                println!("{}", invite_code.code);
            };
        };
        Ok(())
    }
}

/// Manage accounts
#[derive(Subcommand)]
pub enum AccountCommand {
    Create(CreateAccount),
    List(ListAccounts),
    Password(SetPassword),
    Role(SetRole),
    Logout(RevokeOauthTokens),
}

impl AccountCommand {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        match self {
            Self::Create(command) => command.execute(config, db_pool).await,
            Self::List(command) => command.execute(db_pool).await,
            Self::Password(command) => command.execute(db_pool).await,
            Self::Role(command) => command.execute(db_pool).await,
            Self::Logout(command) => command.execute(db_pool).await,
        }
    }
}

/// Manage invite codes
#[derive(Subcommand)]
pub enum InviteCommand {
    Create(GenerateInviteCode),
    List(ListInviteCodes),
}

impl InviteCommand {
    pub async fn execute(
        self,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        match self {
            Self::Create(command) => command.execute(db_pool).await,
            Self::List(command) => command.execute(db_pool).await,
        }
    }
}
