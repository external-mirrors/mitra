use anyhow::{anyhow, Error};
use clap::{CommandFactory, Parser};
use clap_complete::{
    generate,
    shells::Shell,
    Generator,
};
use log::Level;

use mitra_adapters::{
    media::{delete_files, delete_orphaned_media},
};
use mitra_config::Config;
use mitra_models::{
    attachments::queries::delete_unused_attachments,
    database::{get_database_client, DatabaseConnectionPool},
    media::queries::{find_orphaned_files, get_local_files},
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
    users::queries::{
        create_invite_code,
        get_invite_codes,
    },
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
use mitra_utils::datetime::days_before_now;

use crate::commands::{
    account::{
        CreateAccount,
        CreateSystemAccount,
        ListAccounts,
        SetPassword,
        SetRole,
        RevokeOauthTokens,
    },
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
    invoice::{
        ReopenInvoice,
        RepairInvoice,
        GetPaymentAddress,
    },
    post::{CreatePost, DeletePost, ExportPosts, ImportPosts},
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
    CreateSystemAccount(CreateSystemAccount),
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
    ExportPosts(ExportPosts),
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
    /// Generate shell completions
    Completion {
        #[arg(short, long)]
        shell: Shell,
    },
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

pub fn print_completer<G: Generator>(generator: G) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_owned();

    generate(generator, &mut cmd, name, &mut std::io::stdout());
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use super::*;

    #[test]
    fn test_cli() {
        Cli::command().debug_assert();
    }
}
