use clap::{Command as ClapCommand, Parser};
use clap_complete::{
    generate,
    shells::Shell,
    Generator,
};

use crate::commands::{
    account::{
        AccountCommand,
        CreateAccount,
        CreateSystemAccount,
        GenerateInviteCode,
        InviteCommand,
        ListAccounts,
        ListInviteCodes,
        SetPassword,
        SetRole,
        RevokeOauthTokens,
    },
    activitypub::{
        ApCommand,
        CreateActivity,
        FetchObject,
        ImportObject,
        LoadPortableObject,
        LoadReplies,
        SendActivity,
        Webfinger,
    },
    config::{
        ConfigCommand,
        GetConfig,
        UpdateConfig,
    },
    emoji::{
        AddEmoji,
        DeleteEmoji,
        EmojiCommand,
        ImportEmoji,
    },
    filter::{
        AddFilterRule,
        FilterCommand,
        ListFilterRules,
        RemoveFilterRule,
    },
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
        MediaCommand,
        PruneReposts,
    },
};

#[derive(Parser)]
pub enum Command {
    /// Start HTTP server
    Server,
    #[command(hide = true)]
    Worker(Worker),

    #[command(subcommand, hide = true)]
    Account(AccountCommand),
    #[command(subcommand, hide = true)]
    Ap(ApCommand),
    #[command(subcommand, hide = true)]
    Config(ConfigCommand),
    #[command(subcommand, hide = true)]
    Emoji(EmojiCommand),
    #[command(subcommand, hide = true)]
    Filter(FilterCommand),
    #[command(subcommand, hide = true)]
    Invite(InviteCommand),
    #[command(subcommand, hide = true)]
    Media(MediaCommand),

    GetConfig(GetConfig),
    UpdateConfig(UpdateConfig),
    AddFilterRule(AddFilterRule),
    RemoveFilterRule(RemoveFilterRule),
    ListFilterRules(ListFilterRules),
    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    #[command(visible_alias = "create-user")]
    CreateAccount(CreateAccount),
    #[command(hide = true)]
    CreateSystemAccount(CreateSystemAccount),
    #[command(visible_alias = "list-users")]
    ListAccounts(ListAccounts),
    SetPassword(SetPassword),
    SetRole(SetRole),
    RevokeOauthTokens(RevokeOauthTokens),
    ImportObject(ImportObject),
    #[command(visible_alias = "fetch-replies")]
    LoadReplies(LoadReplies),
    FetchObject(FetchObject),
    Webfinger(Webfinger),
    LoadPortableObject(LoadPortableObject),
    CreateActivity(CreateActivity),
    SendActivity(SendActivity),
    #[command(visible_alias = "delete-account", alias = "delete-profile")]
    DeleteUser(DeleteUser),
    CreatePost(CreatePost),
    ImportPosts(ImportPosts),
    ExportPosts(ExportPosts),
    DeletePost(DeletePost),
    AddEmoji(AddEmoji),
    #[command(visible_alias = "steal-emoji")]
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

pub fn print_completer(
    generator: impl Generator,
    cmd: &mut ClapCommand,
) {
    let name = cmd.get_name().to_owned();
    generate(generator, cmd, name, &mut std::io::stdout());
}
