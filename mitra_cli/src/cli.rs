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
    fts::FtsCommand,
    invoice::{
        InvoiceCommand,
        ReopenInvoice,
        RepairInvoice,
        GetPaymentAddress,
    },
    media::{
        DeleteOrphanedFiles,
        ListLocalFiles,
        MediaCommand,
    },
    monero::{
        CreateMoneroSignature,
        CreateMoneroWallet,
        ListActiveAddresses,
        MoneroCommand,
        VerifyMoneroSignature,
    },
    post::{
        CreatePost,
        DeletePost,
        ExportPosts,
        ImportPosts,
        PostCommand,
    },
    process::Worker,
    profile::{
        DeleteUser,
        ListUnreachableActors,
        UserCommand,
    },
    report::InstanceReport,
    storage::{
        CheckUris,
        DeleteEmptyProfiles,
        DeleteExtraneousPosts,
        DeleteUnusedAttachments,
        PruneCommand,
        PruneReposts,
    },
};

#[derive(Parser)]
pub enum Command {
    /// Start HTTP server
    Server,
    #[command(hide = true)]
    Worker(Worker),
    Status(InstanceReport),

    // Command groups
    #[command(subcommand)]
    Account(AccountCommand),
    #[command(subcommand)]
    Ap(ApCommand),
    #[command(subcommand)]
    Config(ConfigCommand),
    #[command(subcommand)]
    Emoji(EmojiCommand),
    #[command(subcommand)]
    Filter(FilterCommand),
    #[command(subcommand)]
    Fts(FtsCommand),
    #[command(subcommand)]
    Invite(InviteCommand),
    #[command(subcommand)]
    Invoice(InvoiceCommand),
    #[command(subcommand)]
    Media(MediaCommand),
    #[command(subcommand)]
    Monero(MoneroCommand),
    #[command(subcommand)]
    Post(PostCommand),
    #[command(subcommand)]
    Prune(PruneCommand),
    #[command(subcommand)]
    User(UserCommand),

    // Dev commands (hidden)
    #[command(hide = true)]
    CheckUris(CheckUris),

    // Deprecated commands (moved to groups)
    #[command(hide = true)]
    GetConfig(GetConfig),
    #[command(hide = true)]
    UpdateConfig(UpdateConfig),
    #[command(hide = true)]
    AddFilterRule(AddFilterRule),
    #[command(hide = true)]
    RemoveFilterRule(RemoveFilterRule),
    #[command(hide = true)]
    ListFilterRules(ListFilterRules),
    #[command(hide = true)]
    GenerateInviteCode(GenerateInviteCode),
    #[command(hide = true)]
    ListInviteCodes(ListInviteCodes),
    #[command(hide = true, visible_alias = "create-user")]
    CreateAccount(CreateAccount),
    #[command(hide = true)]
    CreateSystemAccount(CreateSystemAccount),
    #[command(hide = true, visible_alias = "list-users")]
    ListAccounts(ListAccounts),
    #[command(hide = true)]
    SetPassword(SetPassword),
    #[command(hide = true)]
    SetRole(SetRole),
    #[command(hide = true)]
    RevokeOauthTokens(RevokeOauthTokens),
    #[command(hide = true)]
    ImportObject(ImportObject),
    #[command(hide = true)]
    FetchObject(FetchObject),
    #[command(hide = true)]
    Webfinger(Webfinger),
    #[command(hide = true)]
    CreateActivity(CreateActivity),
    #[command(hide = true)]
    SendActivity(SendActivity),
    #[command(hide = true, visible_alias = "delete-account", alias = "delete-profile")]
    DeleteUser(DeleteUser),
    #[command(hide = true)]
    CreatePost(CreatePost),
    #[command(hide = true)]
    ImportPosts(ImportPosts),
    #[command(hide = true)]
    ExportPosts(ExportPosts),
    #[command(hide = true)]
    DeletePost(DeletePost),
    #[command(hide = true)]
    AddEmoji(AddEmoji),
    #[command(hide = true, visible_alias = "steal-emoji")]
    ImportEmoji(ImportEmoji),
    #[command(hide = true)]
    DeleteEmoji(DeleteEmoji),
    #[command(hide = true)]
    DeleteExtraneousPosts(DeleteExtraneousPosts),
    #[command(hide = true)]
    PruneReposts(PruneReposts),
    #[command(hide = true)]
    DeleteUnusedAttachments(DeleteUnusedAttachments),
    #[command(hide = true)]
    DeleteEmptyProfiles(DeleteEmptyProfiles),
    #[command(hide = true)]
    ListLocalFiles(ListLocalFiles),
    #[command(hide = true)]
    DeleteOrphanedFiles(DeleteOrphanedFiles),
    #[command(hide = true)]
    ListUnreachableActors(ListUnreachableActors),
    #[command(hide = true)]
    CreateMoneroWallet(CreateMoneroWallet),
    #[command(hide = true)]
    ReopenInvoice(ReopenInvoice),
    #[command(hide = true)]
    RepairInvoice(RepairInvoice),
    #[command(hide = true)]
    ListActiveAddresses(ListActiveAddresses),
    #[command(hide = true)]
    InstanceReport(InstanceReport),

    // Deprecated commands (no alias in groups)
    #[command(hide = true, alias = "fetch-replies")]
    LoadReplies(LoadReplies),
    #[command(hide = true)]
    LoadPortableObject(LoadPortableObject),
    #[command(hide = true)]
    CreateMoneroSignature(CreateMoneroSignature),
    #[command(hide = true)]
    VerifyMoneroSignature(VerifyMoneroSignature),
    #[command(hide = true)]
    GetPaymentAddress(GetPaymentAddress),

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
