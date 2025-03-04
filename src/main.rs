use mitra::server::run_server;
use mitra_adapters::init::{
    apply_custom_migrations,
    check_postgres_version,
    initialize_app,
    prepare_instance_keys,
};
use mitra_models::{
    database::{
        connect::create_pool,
        get_database_client,
        migrate::apply_migrations,
    },
};
use mitra_services::media::MediaStorage;
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
    check_postgres_version(&**db_client).await
        .expect("failed to verify PostgreSQL version");
    apply_migrations(&mut db_client).await
        .expect("failed to apply migrations");
    apply_custom_migrations(&**db_client).await
        .expect("failed to apply custom migrations");
    prepare_instance_keys(&mut config, &**db_client).await
        .expect("failed to prepare instance keys");

    let media_storage = MediaStorage::new(&config);
    match media_storage {
        MediaStorage::Filesystem(ref backend) => {
            backend.init().expect("failed to create media directory");
        },
    };

    std::mem::drop(db_client);

    log::info!("instance URL {}", config.instance_url());

    start_workers(config.clone(), db_pool.clone());

    run_server(config, db_pool, media_storage).await
}
