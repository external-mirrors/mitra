use clap::{CommandFactory, Parser};
use clap_complete::{
    generate,
    shells::Shell,
    Generator,
};
use log::Level;

use crate::commands::{
    account::{
        CreateAccount,
        CreateSystemAccount,
        GenerateInviteCode,
        ListAccounts,
        ListInviteCodes,
        SetPassword,
        SetRole,
        RevokeOauthTokens,
    },
    activitypub::{
        CreateActivity,
        FetchObject,
        ImportObject,
        LoadPortableObject,
        LoadReplies,
        SendActivity,
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
    monero::{
        ListActiveAddresses,
        CreateMoneroSignature,
        CreateMoneroWallet,
        VerifyMoneroSignature,
    },
    post::{CreatePost, DeletePost, ExportPosts, ImportPosts},
    process::Worker,
    profile::{
        DeleteUser,
        ListUnreachableActors,
    },
    report::InstanceReport,
    storage::{
        CheckUris,
        DeleteEmptyProfiles,
        DeleteExtraneousPosts,
        DeleteOrphanedFiles,
        DeleteUnusedAttachments,
        ListLocalFiles,
        PruneReposts,
    },
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
    LoadReplies(LoadReplies),
    FetchObject(FetchObject),
    Webfinger(Webfinger),
    LoadPortableObject(LoadPortableObject),
    CreateActivity(CreateActivity),
    SendActivity(SendActivity),
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
