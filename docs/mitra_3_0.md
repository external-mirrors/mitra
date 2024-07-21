# Mitra 3.0 migration guide

## Preparation

If you're using Mitra 1.x, read the [Mitra 2.0 migration guide](./mitra_2_0.md). Updating from 1.x to 3.x is not supported.

## Dependencies

- Minimum supported version of PostgreSQL is 15.
- Minimum supported version of Rust is 1.75.

## Configuration

- Changed the default value of `federation.fetcher_timeout` configuration parameter to `60`. Operators of Tor and I2P instances may need to use a bigger value (e.g. `120`).
- Removed `daemon_url` alias of `node_url` parameter in Monero integration configuration.
- Removed `wallet_url` alias of `wallet_rpc_url` parameter in Monero integration configuration.

## Web client

- Minimum supported version of NodeJS is 18.
