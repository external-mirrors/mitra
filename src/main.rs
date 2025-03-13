use mitra::server::run_server;
use mitra_adapters::init::{
    create_database_client,
    create_database_connection_pool,
    initialize_app,
    initialize_database,
    initialize_storage,
};
use mitra_workers::workers::start_workers;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut config = initialize_app(None);
    let mut db_client = create_database_client(&config).await;
    initialize_database(&mut config, &mut db_client).await;
    initialize_storage(&config);
    log::info!("instance URL {}", config.instance_url());
    std::mem::drop(db_client);

    let db_pool = create_database_connection_pool(&config);
    start_workers(config.clone(), db_pool.clone());
    run_server(config, db_pool).await
}
