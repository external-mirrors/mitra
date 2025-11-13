# Mitra 4.0 migration guide

## Preparation

If you're using Mitra 2.x, read the [Mitra 3.0 migration guide](./mitra_3_0.md). Updating from 2.x to 4.x is not supported.

## General

- The `mitractl` binary was renamed to `mitra`. HTTP server can be started with `mitra server`.
- Removed unhashed OAuth tokens from the database. Some users might be logged out as a result.

## Configuration

- Not reading instance RSA key from `instance_rsa_key` file.
- Default configuration file path doesn't depend on `production` compilation flag anymore. The path is `config.yaml` (relative to current directory), but can be changed with `DEFAULT_CONFIG_PATH` environment variable at compile time. Debian package still installs configuration file to `/etc/mitra/config.yaml`.
- Pruning of remote posts and profiles is enabled by default. Default value of `retention.extraneous_posts` configuration parameter is set to `15` and default value of `retention.empty_profiles` configuration parameter is set to `30`. To disable pruning, set those parameters to `null`.
- Changed default value of `federation.fetcher_timeout` to `30`.
- Changed default value of `federation.deliverer_timeout` to `10`.
- Deprecated `blocked_instances` and `allowed_instances` configuration parameters. Use [federation filter](./filter.md) instead.

## CLI

- Commands don't panic on non-critical errors anymore. Exit code changed from `101` to `1`.
- Renamed `reject` filter action to `reject-incoming`, renamed `reject-data` filter action to `reject`.
- Removed `import-actor` and `import-activity` commands. Use `import-object` instead.
- Changed default value of `object-type` parameter of `import-object` command to `any`.
- Removed `prune-remote-emojis` command.

## Web client

- Minimum supported version of NodeJS is 20.
