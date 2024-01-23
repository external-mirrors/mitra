use clap::Parser;

use mitra_models::database::{
    create_database_client,
    migrate::apply_migrations,
};
use mitra::init::initialize_app;

mod cli;
use cli::{Cli, SubCommand};

#[tokio::main]
async fn main() {
    let opts: Cli = Cli::parse();

    match opts.subcmd {
        SubCommand::GenerateRsaKey(cmd) => cmd.execute().unwrap(),
        SubCommand::GenerateEthereumAddress(cmd) => cmd.execute(),
        subcmd => {
            // Other commands require initialized app
            let config = initialize_app();

            let db_config = config.database_url.parse().unwrap();

            let db_client = &mut create_database_client(
                &db_config,
                config.database_tls_ca_file.as_deref(),
            ).await;
            apply_migrations(db_client).await
                .expect("failed to apply migrations");

            match subcmd {
                SubCommand::GenerateInviteCode(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::ListInviteCodes(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::CreateUser(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ListUsers(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::AddEd25519Key(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::SetPassword(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::SetRole(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::FetchActor(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ReadOutbox(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::FetchReplies(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::FetchObjectAs(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteProfile(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeletePost(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteExtraneousPosts(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteUnusedAttachments(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteOrphanedFiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteEmptyProfiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::PruneRemoteEmojis(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ListUnreachableActors(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::AddEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ImportEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::UpdateCurrentBlock(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ResetSubscriptions(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::CreateMoneroWallet(cmd) => cmd.execute(&config).await.unwrap(),
                SubCommand::CreateMoneroSignature(cmd) => cmd.execute(&config).await.unwrap(),
                SubCommand::VerifyMoneroSignature(cmd) => cmd.execute(&config).await.unwrap(),
                SubCommand::ReopenInvoice(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ListActiveAddresses(cmd) => cmd.execute(&config).await.unwrap(),
                SubCommand::GetPaymentAddress(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::InstanceReport(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                _ => unreachable!(),
            };
        },
    };
}
