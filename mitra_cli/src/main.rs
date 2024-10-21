use clap::Parser;

use mitra_adapters::init::{
    apply_custom_migrations,
    check_postgres_version,
    initialize_app,
    prepare_instance_keys,
};
use mitra_models::database::{
    connect::create_database_client,
    migrate::apply_migrations,
};

mod cli;
mod commands;

use cli::{Cli, SubCommand};

#[tokio::main]
async fn main() {
    let opts: Cli = Cli::parse();
    let mut config = initialize_app(Some(opts.log_level));
    let db_config = config.database_url.parse()
        .expect("failed to parse database URL");
    let db_client = &mut create_database_client(
        &db_config,
        config.database_tls_ca_file.as_deref(),
    ).await.expect("failed to connect to database");
    check_postgres_version(db_client).await
        .expect("failed to verify PostgreSQL version");
    apply_migrations(db_client).await
        .expect("failed to apply migrations");
    apply_custom_migrations(db_client).await
        .expect("failed to apply custom migrations");
    prepare_instance_keys(&mut config, db_client).await
        .expect("failed to prepare instance keys");

    #[allow(clippy::unwrap_used)]
    match opts.subcmd {
        SubCommand::UpdateConfig(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::AddFilterRule(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::RemoveFilterRule(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::ListFilterRules(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::GenerateInviteCode(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::ListInviteCodes(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::CreateAccount(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::ListAccounts(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::SetPassword(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::SetRole(cmd) => cmd.execute(db_client).await.unwrap(),
        SubCommand::FetchActor(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::FetchActivity(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::ReadOutbox(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::FetchReplies(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::FetchObject(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::LoadPortableObject(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::DeleteUser(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::DeletePost(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::AddEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::ImportEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::DeleteEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::DeleteExtraneousPosts(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::DeleteUnusedAttachments(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::DeleteEmptyProfiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::PruneRemoteEmojis(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::ListLocalFiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::DeleteOrphanedFiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::ListUnreachableActors(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::CreateMoneroWallet(cmd) => cmd.execute(&config).await.unwrap(),
        SubCommand::CreateMoneroSignature(cmd) => cmd.execute(&config).await.unwrap(),
        SubCommand::VerifyMoneroSignature(cmd) => cmd.execute(&config).await.unwrap(),
        SubCommand::ReopenInvoice(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::ListActiveAddresses(cmd) => cmd.execute(&config).await.unwrap(),
        SubCommand::GetPaymentAddress(cmd) => cmd.execute(&config, db_client).await.unwrap(),
        SubCommand::InstanceReport(cmd) => cmd.execute(&config, db_client).await.unwrap(),
    };
}
