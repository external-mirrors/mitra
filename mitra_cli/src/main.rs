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
    let maybe_override_log_level = if let SubCommand::Server = opts.subcmd {
        // Do not override log level when running server
        None
    } else {
        Some(opts.log_level)
    };
    let mut config = initialize_app(maybe_override_log_level);
    let mut db_client_value = create_database_client(&config).await;
    let db_client = &mut db_client_value;
    initialize_database(&mut config, db_client).await;
    initialize_storage(&config);
    log::info!("instance URL {}", config.instance_url());

    let result = match opts.subcmd {
        SubCommand::Server => {
            std::mem::drop(db_client_value);
            let db_pool = create_database_connection_pool(&config);
            start_workers(config.clone(), db_pool.clone());
            let result = run_server(config, db_pool).await;
            result.map_err(Into::into)
        },
        SubCommand::UpdateConfig(cmd) => cmd.execute(db_client).await,
        SubCommand::AddFilterRule(cmd) => cmd.execute(db_client).await,
        SubCommand::RemoveFilterRule(cmd) => cmd.execute(db_client).await,
        SubCommand::ListFilterRules(cmd) => cmd.execute(db_client).await,
        SubCommand::GenerateInviteCode(cmd) => cmd.execute(db_client).await,
        SubCommand::ListInviteCodes(cmd) => cmd.execute(db_client).await,
        SubCommand::CreateAccount(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ListAccounts(cmd) => cmd.execute(db_client).await,
        SubCommand::SetPassword(cmd) => cmd.execute(db_client).await,
        SubCommand::SetRole(cmd) => cmd.execute(db_client).await,
        SubCommand::RevokeOauthTokens(cmd) => cmd.execute(db_client).await,
        SubCommand::ImportObject(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ReadOutbox(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::LoadReplies(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::FetchObject(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::LoadPortableObject(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::DeleteUser(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::CreatePost(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::DeletePost(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::AddEmoji(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ImportEmoji(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::DeleteEmoji(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::DeleteExtraneousPosts(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::PruneReposts(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::DeleteUnusedAttachments(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::DeleteEmptyProfiles(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ListLocalFiles(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::DeleteOrphanedFiles(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ListUnreachableActors(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::CreateMoneroWallet(cmd) => cmd.execute(&config).await,
        SubCommand::CreateMoneroSignature(cmd) => cmd.execute(&config).await,
        SubCommand::VerifyMoneroSignature(cmd) => cmd.execute(&config).await,
        SubCommand::ReopenInvoice(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ListActiveAddresses(cmd) => cmd.execute(&config).await,
        SubCommand::GetPaymentAddress(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::InstanceReport(cmd) => cmd.execute(&config, db_client).await,
    };
    result
}
