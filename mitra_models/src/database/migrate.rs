use refinery::Error;
use tokio_postgres::Client;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub async fn apply_migrations(db_client: &mut Client) -> Result<(), Error> {
    let runner = embedded::migrations::runner();

    let maybe_last_migration =
        runner.get_last_applied_migration_async(db_client).await;
    if let Ok(Some(migration)) = maybe_last_migration {
        if migration.version() < 72 {
            // Migration v72 was added in 2.13.0
            panic!("updating from versions older than 2.13.0 is not supported");
        };
    };

    let migration_report = runner.run_async(db_client).await?;
    for migration in migration_report.applied_migrations() {
        log::info!(
            "migration applied: version {} ({})",
            migration.version(),
            migration.name(),
        );
    };
    Ok(())
}
