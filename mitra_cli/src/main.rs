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
async fn main() -> () {
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

    let result = match opts.subcmd {
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
        SubCommand::ImportActor(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ImportObject(cmd) => cmd.execute(&config, db_client).await,
        SubCommand::ImportActivity(cmd) => cmd.execute(&config, db_client).await,
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
        SubCommand::PruneRemoteEmojis(cmd) => cmd.execute(&config, db_client).await,
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
    #[allow(clippy::unwrap_used)]
    result.unwrap()
}
