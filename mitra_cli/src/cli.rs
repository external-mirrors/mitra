use anyhow::{anyhow, Error};
use apx_core::{
    crypto::{
        eddsa::generate_ed25519_key,
        rsa::{
            generate_rsa_key,
            rsa_secret_key_to_pkcs8_pem,
        },
    },
};
use clap::Parser;
use log::Level;
use uuid::Uuid;

use mitra_adapters::{
    media::{delete_files, delete_orphaned_media},
    payments::monero::{
        get_payment_address,
        reopen_local_invoice,
    },
    roles::{
        from_default_role,
        role_from_str,
        role_to_str,
        ALLOWED_ROLES,
    },
};
use mitra_config::Config;
use mitra_models::{
    attachments::queries::delete_unused_attachments,
    database::{get_database_client, DatabaseConnectionPool},
    invoices::{
        queries::{
            get_local_invoice_by_address,
            get_invoice_by_id,
        },
    },
    media::queries::{find_orphaned_files, get_local_files},
    oauth::queries::delete_oauth_tokens,
    posts::queries::{
        delete_post,
        find_extraneous_posts,
    },
    profiles::queries::{
        delete_profile,
        find_empty_profiles,
        find_unreachable,
        get_profile_by_id,
    },
    users::helpers::get_user_by_id_or_name,
    users::queries::{
        create_invite_code,
        create_user,
        get_accounts_for_admin,
        get_invite_codes,
        set_user_password,
        set_user_role,
    },
    users::types::UserCreateData,
};
use mitra_services::{
    media::MediaStorage,
    monero::{
        wallet::{
            create_monero_signature,
            create_monero_wallet,
            get_active_addresses,
            open_monero_wallet,
            verify_monero_signature,
        },
    },
};
use mitra_utils::{
    datetime::days_before_now,
    passwords::hash_password,
};
use mitra_validators::users::validate_local_username;

use crate::commands::{
    account::RevokeOauthTokens,
    activitypub::{
        FetchObject,
        ImportObject,
        LoadPortableObject,
        LoadReplies,
        ReadOutbox,
        Webfinger,
    },
    config::{GetConfig, UpdateConfig},
    emoji::{AddEmoji, DeleteEmoji, ImportEmoji},
    filter::{AddFilterRule, ListFilterRules, RemoveFilterRule},
    invoice::RepairInvoice,
    post::{CreatePost, DeletePost, ImportPosts},
    process::Worker,
    profile::DeleteUser,
    report::InstanceReport,
    storage::{CheckUris, PruneReposts},
};

/// Mitra admin CLI
#[derive(Parser)]
#[command(version)]
pub struct Cli {
    #[arg(long, default_value_t = Level::Warn)]
    pub log_level: Level,

    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    /// Start HTTP server
    Server,
    Worker(Worker),
    GetConfig(GetConfig),
    UpdateConfig(UpdateConfig),
    AddFilterRule(AddFilterRule),
    RemoveFilterRule(RemoveFilterRule),
    ListFilterRules(ListFilterRules),
    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    CreateAccount(CreateAccount),
    ListAccounts(ListAccounts),
    SetPassword(SetPassword),
    SetRole(SetRole),
    RevokeOauthTokens(RevokeOauthTokens),
    ImportObject(ImportObject),
    ReadOutbox(ReadOutbox),
    LoadReplies(LoadReplies),
    FetchObject(FetchObject),
    Webfinger(Webfinger),
    LoadPortableObject(LoadPortableObject),
    DeleteUser(DeleteUser),
    CreatePost(CreatePost),
    ImportPosts(ImportPosts),
    DeletePost(DeletePost),
    AddEmoji(AddEmoji),
    ImportEmoji(ImportEmoji),
    DeleteEmoji(DeleteEmoji),
    DeleteExtraneousPosts(DeleteExtraneousPosts),
    PruneReposts(PruneReposts),
    DeleteUnusedAttachments(DeleteUnusedAttachments),
    DeleteEmptyProfiles(DeleteEmptyProfiles),
    ListLocalFiles(ListLocalFiles),
    DeleteOrphanedFiles(DeleteOrphanedFiles),
    ListUnreachableActors(ListUnreachableActors),
    CheckUris(CheckUris),
    CreateMoneroWallet(CreateMoneroWallet),
    CreateMoneroSignature(CreateMoneroSignature),
    VerifyMoneroSignature(VerifyMoneroSignature),
    ReopenInvoice(ReopenInvoice),
    RepairInvoice(RepairInvoice),
    ListActiveAddresses(ListActiveAddresses),
    GetPaymentAddress(GetPaymentAddress),
    InstanceReport(InstanceReport),
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

/// Delete old remote posts
#[derive(Parser)]
pub struct DeleteExtraneousPosts {
    days: u32,
}

impl DeleteExtraneousPosts {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let updated_before = days_before_now(self.days);
        let posts = find_extraneous_posts(db_client, updated_before).await?;
        for post_id in posts {
            let deletion_queue = delete_post(db_client, post_id).await?;
            delete_orphaned_media(config, db_client, deletion_queue).await?;
            println!("post {} deleted", post_id);
        };
        Ok(())
    }
}

/// Delete attachments that don't belong to any post
#[derive(Parser)]
pub struct DeleteUnusedAttachments {
    days: u32,
}

impl DeleteUnusedAttachments {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let created_before = days_before_now(self.days);
        let (deleted_count, deletion_queue) = delete_unused_attachments(
            db_client,
            created_before,
        ).await?;
        delete_orphaned_media(config, db_client, deletion_queue).await?;
        println!("deleted {deleted_count} unused attachments");
        Ok(())
    }
}

/// Delete empty remote profiles
#[derive(Parser)]
pub struct DeleteEmptyProfiles {
    days: u32,
}

impl DeleteEmptyProfiles {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let updated_before = days_before_now(self.days);
        let profiles = find_empty_profiles(db_client, updated_before).await?;
        for profile_id in profiles {
            let profile = get_profile_by_id(db_client, profile_id).await?;
            let deletion_queue = delete_profile(db_client, profile.id).await?;
            delete_orphaned_media(config, db_client, deletion_queue).await?;
            println!("profile deleted: {}", profile.expect_remote_actor_id());
        };
        Ok(())
    }
}

/// List files uploaded by local users
#[derive(Parser)]
pub struct ListLocalFiles;

impl ListLocalFiles {
    pub async fn execute(
        self,
        _config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let filenames = get_local_files(db_client).await?;
        for file_name in filenames {
            println!("{file_name}");
        };
        Ok(())
    }
}

/// Find and delete orphaned files
#[derive(Parser)]
pub struct DeleteOrphanedFiles {
    /// List found files, but don't delete them
    #[arg(long)]
    dry_run: bool,
}

impl DeleteOrphanedFiles {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let media_storage = MediaStorage::new(config);
        let files = media_storage.list_files()?;
        let orphaned = find_orphaned_files(db_client, files).await?;
        if orphaned.is_empty() {
            println!("no orphaned files found");
            return Ok(());
        };
        if self.dry_run {
            for file_name in orphaned {
                println!("orphaned file: {file_name}");
            };
        } else {
            delete_files(&media_storage, &orphaned);
            println!("orphaned files deleted: {}", orphaned.len());
        };
        Ok(())
    }
}

/// List unreachable actors
#[derive(Parser)]
pub struct ListUnreachableActors {
    days: u32,
}

impl ListUnreachableActors {
    pub async fn execute(
        self,
        _config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let unreachable_since = days_before_now(self.days);
        let profiles = find_unreachable(db_client, unreachable_since).await?;
        println!(
            "{0: <60} | {1: <35} | {2: <35}",
            "ID", "unreachable since", "updated at",
        );
        for profile in profiles {
            println!(
                "{0: <60} | {1: <35} | {2: <35}",
                profile.expect_remote_actor_id(),
                profile.unreachable_since
                    .expect("unreachable flag should be present")
                    .to_string(),
                profile.updated_at.to_string(),
            );
        };
        Ok(())
    }
}

/// Create Monero wallet
/// (can be used when monero-wallet-rpc runs with --wallet-dir option)
#[derive(Parser)]
pub struct CreateMoneroWallet {
    name: String,
    password: Option<String>,
}

impl CreateMoneroWallet {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        create_monero_wallet(
            monero_config,
            self.name,
            self.password,
        ).await?;
        println!("wallet created");
        Ok(())
    }
}

/// Create Monero signature
#[derive(Parser)]
pub struct CreateMoneroSignature {
    message: String,
}

impl CreateMoneroSignature {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let (address, signature) =
            create_monero_signature(monero_config, &self.message).await?;
        println!("address: {}", address);
        println!("signature: {}", signature);
        Ok(())
    }
}

/// Verify Monero signature
#[derive(Parser)]
pub struct VerifyMoneroSignature {
    address: String,
    message: String,
    signature: String,
}

impl VerifyMoneroSignature {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        verify_monero_signature(
            monero_config,
            &self.address,
            &self.message,
            &self.signature,
        ).await?;
        println!("signature verified");
        Ok(())
    }
}

/// Re-open closed invoice (already processed, timed out or cancelled)
#[derive(Parser)]
pub struct ReopenInvoice {
    id_or_address: String,
}

impl ReopenInvoice {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let invoice = if let Ok(invoice_id) = Uuid::parse_str(&self.id_or_address) {
            get_invoice_by_id(db_client, invoice_id).await?
        } else {
            get_local_invoice_by_address(
                db_client,
                &monero_config.chain_id,
                &self.id_or_address,
            ).await?
        };
        reopen_local_invoice(
            monero_config,
            db_client,
            &invoice,
        ).await?;
        Ok(())
    }
}

#[derive(Parser)]
pub struct ListActiveAddresses;

impl ListActiveAddresses {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let wallet_client = open_monero_wallet(monero_config).await?;
        let addresses = get_active_addresses(
            &wallet_client,
            monero_config.account_index,
        ).await?;
        for (address, amount) in addresses {
            println!("{}: {}", address, amount);
        };
        Ok(())
    }
}

/// Get payment address for given sender and recipient
#[derive(Parser)]
pub struct GetPaymentAddress {
    sender_id: Uuid,
    recipient_id: Uuid,
}

impl GetPaymentAddress {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &mut **get_database_client(db_pool).await?;
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let payment_address = get_payment_address(
            monero_config,
            db_client,
            self.sender_id,
            self.recipient_id,
        ).await?;
        println!("payment address: {}", payment_address);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use super::*;

    #[test]
    fn test_cli() {
        Cli::command().debug_assert()
    }
}
