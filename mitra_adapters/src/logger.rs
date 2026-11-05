use std::io::Write;

use log::{Level, LevelFilter};
use chrono::Local;

// Next level (less verbose)
fn next_level(level: Level) -> Level {
    Level::iter()
        .filter(|item| *item < level)
        .last()
        .unwrap_or(Level::Error)
}

pub fn configure_logger(base_level: Level) -> () {
    let actix_level = next_level(base_level);
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(buf,
                "{} {} [{}] {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.target(),
                record.level(),
                record.args(),
            )
        })
        .filter_level(base_level.to_level_filter())
        .filter_module("actix_web::middleware::logger", actix_level.to_level_filter())
        // [ERROR] failed to verify TLS certificate: invalid peer certificate
        // TODO: remove when fixed upstream
        .filter_module(
            "rustls_platform_verifier::verification::others",
            LevelFilter::Off,
        )
        .parse_default_env()
        .init();
}
