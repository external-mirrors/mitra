# Mitra 2.0 migration guide

## Dependencies

- Minimum supported version of PostgreSQL is 13. Version 12 may work too, but upgrading to a newer version is recommended.
- Recommended version of Rust is 1.63 (this is the version of `rustc` package in Debian 12 Bookworm). Mitra 2.0 can be compiled using Rust 1.57, but upgrading to a newer version of the compiler is recommended.
- Minimum supported version of Node.js is 16.

## Configuration

- Removed `registrations_open` configuration parameter. Use `registration.type` instead.

Prior to version 2.0, Mitra printed warnings when it encountered deprecated configuration parameters. In Mitra 2.0 these parameters are ignored.

See [annotated example of config.yaml file](../contrib/mitra_config.yaml) for more information.
