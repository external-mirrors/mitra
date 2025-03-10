use mitra::server::run_server;
use mitra_adapters::init::{
    initialize_app,
    initialize_database,
    initialize_storage,
};
use mitra_models::{
    database::{
        connect::create_pool,
        get_database_client,
    },
};
use mitra_workers::workers::start_workers;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let mut config = initialize_app(None);

    // https://wiki.postgresql.org/wiki/Number_Of_Database_Connections
    // https://docs.rs/deadpool/0.10.0/src/deadpool/managed/config.rs.html#54
    let db_pool_size = num_cpus::get_physical() * 2;
    log::info!("database connection pool size: {db_pool_size}");
    let db_pool = create_pool(
        &config.database_url,
        config.database_tls_ca_file.as_deref(),
        db_pool_size,
    ).expect("failed to connect to database");
    let mut db_client = get_database_client(&db_pool).await
        .expect("failed to connect to database");
    initialize_database(&mut config, &mut db_client).await;
    initialize_storage(&config);
    std::mem::drop(db_client);

    log::info!("instance URL {}", config.instance_url());

    start_workers(config.clone(), db_pool.clone());

    run_server(config, db_pool).await
}
