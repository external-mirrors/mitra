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
    Fts(FtsCommand),
    #[command(subcommand, hide = true)]
    Invite(InviteCommand),
    #[command(subcommand, hide = true)]
    Invoice(InvoiceCommand),
    #[command(subcommand, hide = true)]
    Media(MediaCommand),
    #[command(subcommand, hide = true)]
    Monero(MoneroCommand),
    #[command(subcommand, hide = true)]
    Post(PostCommand),
    #[command(subcommand, hide = true)]
    Prune(PruneCommand),
    #[command(subcommand, hide = true)]
    User(UserCommand),

    // Dev commands (hidden)
    #[command(hide = true)]
    CheckUris(CheckUris),

    // Legacy commands
    GetConfig(GetConfig),
    UpdateConfig(UpdateConfig),
    AddFilterRule(AddFilterRule),
    RemoveFilterRule(RemoveFilterRule),
    ListFilterRules(ListFilterRules),
    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    #[command(visible_alias = "create-user")]
    CreateAccount(CreateAccount),
    CreateSystemAccount(CreateSystemAccount),
    #[command(visible_alias = "list-users")]
    ListAccounts(ListAccounts),
    SetPassword(SetPassword),
    SetRole(SetRole),
    RevokeOauthTokens(RevokeOauthTokens),
    ImportObject(ImportObject),
    FetchObject(FetchObject),
    Webfinger(Webfinger),
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
    CreateMoneroWallet(CreateMoneroWallet),
    ReopenInvoice(ReopenInvoice),
    RepairInvoice(RepairInvoice),
    ListActiveAddresses(ListActiveAddresses),
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
