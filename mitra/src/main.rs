use anyhow::Error;
use clap::Parser;
use tokio::runtime::Builder;

use mitra_adapters::init::{
    check_app_directories,
    create_database_client,
    create_database_connection_pool,
    initialize_app,
    initialize_database,
    initialize_storage,
};
use mitra_api::server::run_server;
use mitra_config::SoftwareMetadata;
use mitra_cli::cli::{
    print_completer,
    Cli,
    Command,
};
use mitra_workers::workers::start_workers;

fn get_software_metadata() -> SoftwareMetadata {
    SoftwareMetadata {
        name: "Mitra",
        version: env!("CARGO_PKG_VERSION"),
        repository: "https://codeberg.org/silverpill/mitra",
    }
}

async fn run_async() -> Result<(), Error> {
    let opts: Cli = Cli::parse();

    if let Command::Completion { shell } = opts.command {
        print_completer(shell);
        return Ok(());
    };

    let maybe_override_log_level = match opts.command {
        Command::Server | Command::Worker(_) => {
            // Do not override log level when running a process
            None
        },
        _ => {
            Some(opts.log_level)
        },
    };
    let mut config = initialize_app(
        get_software_metadata(),
        maybe_override_log_level,
    );
    check_app_directories(&config);
    let mut db_client_value = create_database_client(&config).await;
    let db_client = &mut db_client_value;
    initialize_database(&mut config, db_client).await;
    initialize_storage(&config);
    log::info!("instance URL {}", config.instance().uri());
    std::mem::drop(db_client_value);

    let db_pool = create_database_connection_pool(&config);
    let result = match opts.command {
        Command::Server => {
            start_workers(config.clone(), db_pool.clone());
            let result = run_server(config, db_pool).await;
            result.map_err(Into::into)
        },
        Command::Worker(cmd) => cmd.execute(config, db_pool).await,
        Command::Account(command) => command.execute(&config, &db_pool).await,
        Command::Ap(command) => command.execute(&config, &db_pool).await,
        Command::Config(command) => command.execute(&db_pool).await,
        Command::Emoji(command) => command.execute(&config, &db_pool).await,
        Command::Filter(command) => command.execute(&db_pool).await,
        Command::Invite(command) => command.execute(&db_pool).await,
        Command::Media(command) => command.execute(&config, &db_pool).await,
        Command::GetConfig(cmd) => cmd.execute(&db_pool).await,
        Command::UpdateConfig(cmd) => cmd.execute(&db_pool).await,
        Command::AddFilterRule(cmd) => cmd.execute(&db_pool).await,
        Command::RemoveFilterRule(cmd) => cmd.execute(&db_pool).await,
        Command::ListFilterRules(cmd) => cmd.execute(&db_pool).await,
        Command::GenerateInviteCode(cmd) => cmd.execute(&db_pool).await,
        Command::ListInviteCodes(cmd) => cmd.execute(&db_pool).await,
        Command::CreateAccount(cmd) => cmd.execute(&config, &db_pool).await,
        Command::CreateSystemAccount(cmd) => cmd.execute(&config, &db_pool).await,
        Command::ListAccounts(cmd) => cmd.execute(&db_pool).await,
        Command::SetPassword(cmd) => cmd.execute(&db_pool).await,
        Command::SetRole(cmd) => cmd.execute(&db_pool).await,
        Command::RevokeOauthTokens(cmd) => cmd.execute(&db_pool).await,
        Command::ImportObject(cmd) => cmd.execute(&config, &db_pool).await,
        Command::LoadReplies(cmd) => cmd.execute(&config, &db_pool).await,
        Command::FetchObject(cmd) => cmd.execute(&config, &db_pool).await,
        Command::Webfinger(cmd) => cmd.execute(&config, &db_pool).await,
        Command::LoadPortableObject(cmd) => cmd.execute(&config, &db_pool).await,
        Command::CreateActivity(cmd) => cmd.execute(&config, &db_pool).await,
        Command::SendActivity(cmd) => cmd.execute(&config, &db_pool).await,
        Command::DeleteUser(cmd) => cmd.execute(&config, &db_pool).await,
        Command::CreatePost(cmd) => cmd.execute(&config, &db_pool).await,
        Command::ImportPosts(cmd) => cmd.execute(&config, &db_pool).await,
        Command::ExportPosts(cmd) => cmd.execute(&config, &db_pool).await,
        Command::DeletePost(cmd) => cmd.execute(&config, &db_pool).await,
        Command::AddEmoji(cmd) => cmd.execute(&config, &db_pool).await,
        Command::ImportEmoji(cmd) => cmd.execute(&config, &db_pool).await,
        Command::DeleteEmoji(cmd) => cmd.execute(&config, &db_pool).await,
        Command::DeleteExtraneousPosts(cmd) => cmd.execute(&config, &db_pool).await,
        Command::PruneReposts(cmd) => cmd.execute(&config, &db_pool).await,
        Command::DeleteUnusedAttachments(cmd) => cmd.execute(&config, &db_pool).await,
        Command::DeleteEmptyProfiles(cmd) => cmd.execute(&config, &db_pool).await,
        Command::ListLocalFiles(cmd) => cmd.execute(&config, &db_pool).await,
        Command::DeleteOrphanedFiles(cmd) => cmd.execute(&config, &db_pool).await,
        Command::ListUnreachableActors(cmd) => cmd.execute(&config, &db_pool).await,
        Command::CheckUris(cmd) => cmd.execute(&config, &db_pool).await,
        Command::CreateMoneroWallet(cmd) => cmd.execute(&config).await,
        Command::CreateMoneroSignature(cmd) => cmd.execute(&config).await,
        Command::VerifyMoneroSignature(cmd) => cmd.execute(&config).await,
        Command::ReopenInvoice(cmd) => cmd.execute(&config, &db_pool).await,
        Command::RepairInvoice(cmd) => cmd.execute(&config, &db_pool).await,
        Command::ListActiveAddresses(cmd) => cmd.execute(&config).await,
        Command::GetPaymentAddress(cmd) => cmd.execute(&config, &db_pool).await,
        Command::InstanceReport(cmd) => cmd.execute(&config, &db_pool).await,
        Command::Completion { .. } => unreachable!(),
    };
    result
}

fn main() -> Result<(), Error> {
    Builder::new_multi_thread()
        .enable_all()
        // The default stack size is 2 MB,
        // which is not enough for background workers
        .thread_stack_size(4_000_000)
        .build()
        .expect("runtime options should be correct")
        .block_on(run_async())
}
