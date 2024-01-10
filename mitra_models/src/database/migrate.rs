use refinery::Error;
use tokio_postgres::Client;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub async fn apply_migrations(db_client: &mut Client) -> Result<(), Error> {
    let migration_report = embedded::migrations::runner()
        .run_async(db_client)
        .await?;

    for migration in migration_report.applied_migrations() {
        log::info!(
            "migration applied: version {} ({})",
            migration.version(),
            migration.name(),
        );
    };
    Ok(())
}
