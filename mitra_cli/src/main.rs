use anyhow::Error;
use clap::Parser;

use mitra_adapters::init::{
    create_database_client,
    create_database_connection_pool,
    initialize_app,
    initialize_database,
    initialize_storage,
};
use mitra_api::server::run_server;
use mitra_workers::workers::start_workers;

mod cli;
mod commands;

use cli::{Cli, SubCommand};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let opts: Cli = Cli::parse();
    let maybe_override_log_level = match opts.subcmd {
        SubCommand::Server | SubCommand::Worker(_) => {
            // Do not override log level when running a process
            None
        },
        _ => {
            Some(opts.log_level)
        },
    };
    let mut config = initialize_app(maybe_override_log_level);
    let mut db_client_value = create_database_client(&config).await;
    let db_client = &mut db_client_value;
    initialize_database(&mut config, db_client).await;
    initialize_storage(&config);
    log::info!("instance URL {}", config.instance().uri());
    std::mem::drop(db_client_value);

    let db_pool = create_database_connection_pool(&config);
    let result = match opts.subcmd {
        SubCommand::Server => {
            start_workers(config.clone(), db_pool.clone());
            let result = run_server(config, db_pool).await;
            result.map_err(Into::into)
        },
        SubCommand::Worker(cmd) => cmd.execute(config, db_pool).await,
        SubCommand::GetConfig(cmd) => cmd.execute(&db_pool).await,
        SubCommand::UpdateConfig(cmd) => cmd.execute(&db_pool).await,
        SubCommand::AddFilterRule(cmd) => cmd.execute(&db_pool).await,
        SubCommand::RemoveFilterRule(cmd) => cmd.execute(&db_pool).await,
        SubCommand::ListFilterRules(cmd) => cmd.execute(&db_pool).await,
        SubCommand::GenerateInviteCode(cmd) => cmd.execute(&db_pool).await,
        SubCommand::ListInviteCodes(cmd) => cmd.execute(&db_pool).await,
        SubCommand::CreateAccount(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::ListAccounts(cmd) => cmd.execute(&db_pool).await,
        SubCommand::SetPassword(cmd) => cmd.execute(&db_pool).await,
        SubCommand::SetRole(cmd) => cmd.execute(&db_pool).await,
        SubCommand::RevokeOauthTokens(cmd) => cmd.execute(&db_pool).await,
        SubCommand::ImportObject(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::ReadOutbox(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::LoadReplies(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::FetchObject(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::Webfinger(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::LoadPortableObject(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::DeleteUser(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::CreatePost(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::ImportPosts(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::DeletePost(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::AddEmoji(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::ImportEmoji(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::DeleteEmoji(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::DeleteExtraneousPosts(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::PruneReposts(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::DeleteUnusedAttachments(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::DeleteEmptyProfiles(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::ListLocalFiles(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::DeleteOrphanedFiles(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::ListUnreachableActors(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::CheckUris(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::CreateMoneroWallet(cmd) => cmd.execute(&config).await,
        SubCommand::CreateMoneroSignature(cmd) => cmd.execute(&config).await,
        SubCommand::VerifyMoneroSignature(cmd) => cmd.execute(&config).await,
        SubCommand::ReopenInvoice(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::RepairInvoice(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::ListActiveAddresses(cmd) => cmd.execute(&config).await,
        SubCommand::GetPaymentAddress(cmd) => cmd.execute(&config, &db_pool).await,
        SubCommand::InstanceReport(cmd) => cmd.execute(&config, &db_pool).await,
    };
    result
}
