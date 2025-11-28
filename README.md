<div align="center">
  <img src="./logo/logo-black-text.svg" alt="Mitra logo" width="150">
</div>

## About

Federated micro-blogging platform.

Built on [ActivityPub](https://www.w3.org/TR/activitypub/) protocol, self-hosted, lightweight. Part of the [Fediverse](https://en.wikipedia.org/wiki/Fediverse).

Features:

- Micro-blogging service
  - Quote posts, custom emojis, reactions, polls and more.
  - Default character limit is 5000.
  - Support for [markdown-style](./docs/post_markup.md) formatting.
- Easy installation and small memory footprint (<50 MB).
- Interoperable. Can show content from blogs, forums and other types of federated services.
- Mastodon API.
- Content subscription service. Subscriptions provide a way to receive monthly payments from subscribers and to publish private content made exclusively for them.
  - Supported payment methods: [Monero](https://www.getmonero.org/get-started/what-is-monero/), a peer to peer digital cash system where transactions are private by default.
- Account [migrations](./docs/migrations.md) (from one server to another). Identity can be detached from the server.
- Federation over Tor and/or I2P.

Follow: [@mitra@mitra.social](https://mitra.social/@mitra)

Matrix chat: [#mitra:unredacted.org](https://matrix.to/#/#mitra:unredacted.org)

## Instances

- [FediDB](https://fedidb.org/software/mitra)
- [Fediverse Observer](https://mitra.fediverse.observer/list)
- [FediList](https://fedilist.com/instance?software=mitra)

Demo instance: https://public.mitra.social/ ([invite-only](https://public.mitra.social/about))

## Supported clients

- [mitra-web](https://codeberg.org/silverpill/mitra-web) (Web)
- [Bloat](https://git.freesoftwareextremist.com/bloat/about/) (Web, NoJS)
- [Phanpy](https://github.com/cheeaun/phanpy) (Web)
- [pl-fe](https://github.com/mkljczk/pl-fe) (Web)
- [Husky](https://github.com/captainepoch/husky) (Android)
- [Fedilab](https://codeberg.org/tom79/Fedilab) (Android)
- [Fedicat](https://codeberg.org/technicat/fedicat) (iOS)
- [toot](https://github.com/ihabunek/toot) (CLI)

## Requirements

- PostgreSQL 15+
- Rust 1.75+ (when building from source)
- SSL certificates (i.e. `ca-certificates` package).

Minimum system requirements:

- 256 MB RAM (1 GB for building from source)
- 10 GB storage for average single user instance with default configuration

## Installation

### Debian package

Download package from the [Releases](https://codeberg.org/silverpill/mitra/releases) page.

Install Mitra:

```shell
dpkg -i mitra_amd64.deb
```

Install PostgreSQL, then create the database:

```sql
CREATE USER mitra WITH PASSWORD 'mitra';
CREATE DATABASE mitra OWNER mitra ENCODING 'UTF8';
```

Open configuration file `/etc/mitra/config.yaml` and configure the instance.

Create admin account:

```shell
su mitra -s $SHELL -c "mitra create-account <username> <password> admin"
```

Start Mitra:

```shell
systemctl enable --now mitra
```

An HTTP server will be needed to handle HTTPS requests. See examples of [Nginx](./contrib/mitra.nginx) and [Caddy](./docs/reverse_proxy.md#caddy) configuration files.

### Building from source

Clone the git repository or download the source archive from the [Releases](https://codeberg.org/silverpill/mitra/releases) page.

Install `cargo`. Then run:

```shell
cargo build --release --features production
```

This command will produce a `mitra` binary in `target/release` directory.

Install PostgreSQL, then create the database:

```sql
CREATE USER mitra WITH PASSWORD 'mitra';
CREATE DATABASE mitra OWNER mitra ENCODING 'UTF8';
```

Create configuration file by copying [`config.example.yaml`](./config.example.yaml) and configure the instance. Default config file path is `config.yaml`, but it can be changed using `CONFIG_PATH` environment variable.

Create data and web client directories at locations specified in the configuration file (`storage_dir` and `web_client_dir` parameters).

Put any static files into the web client directory. Building instructions for `mitra-web` frontend can be found at https://codeberg.org/silverpill/mitra-web#project-setup.

Create admin account:

```shell
./mitra create-account <username> <password> admin
```

Start Mitra:

```shell
./mitra server
```

An HTTP server will be needed to handle HTTPS requests. See examples of [Nginx](./contrib/mitra.nginx) and [Caddy](./docs/reverse_proxy.md#caddy) configuration files.

To run Mitra as a systemd service, check out the [systemd unit file example](./contrib/mitra.service).

### Other installation methods

These images and packages are maintained by the community.

#### Docker image

https://hub.docker.com/r/bleakfuture0/mitra

#### Alpine Linux

Install from testing repository:

```shell
echo '@testing https://dl-cdn.alpinelinux.org/alpine/edge/testing' >> /etc/apk/repositories
apk update
apk add -vi mitra@testing
```

#### Nix

https://github.com/NixOS/nixpkgs/blob/nixos-unstable/pkgs/by-name/mi/mitra/package.nix

#### YunoHost

https://apps.yunohost.org/app/mitra

## Upgrading

Mitra uses semantic versioning (`major.minor.patch`):

- `patch` - bugfixes
- `minor` - improvements and new features
- `major` - breaking changes

Upgrade to a `minor` or a `patch` version is performed by replacing binaries and restarting the service.

Upgrade to a `major` version requires special migration steps that are documented in release notes.

### Debian package

Install package:

```shell
dpkg -i mitra_amd64.deb
```

The server will be stopped automatically during installation. Do not overwrite existing configuration file if asked.

Start Mitra again when the installation is complete:

```
systemctl start mitra
```

## Configuration

### Environment variables

See [defaults](./.env).

### Tor/I2P federation

See [Tor guide](./docs/onion.md) and [I2P guide](./docs/i2p.md).

### Payments

- [Monero](./docs/monero.md)

*Subscriptions can be used without enabling Monero integration.*

### IPFS integration (experimental)

See [guide](./docs/ipfs.md).

*IPFS integration is not actively maintained and may be removed in the future.*

## Administration

- [Backup and restore](./docs/backup_and_restore.md)
- [Cache management](./docs/cache_management.md)
- [Filter](./docs/filter.md)
- [Relays](./docs/relays.md)
- [Custom themes](./docs/custom_themes.md)
- [Debugging](./docs/debugging.md)

## CLI

CLI is stable and breaking changes don't happen in minor releases.

[Documentation](./docs/mitra_cli.md)

## REST API

The majority of endpoints imitate [Mastodon API](https://docs.joinmastodon.org/client/intro/). Some [Pleroma](https://docs.pleroma.social/backend/development/API/differences_in_mastoapi_responses/) extensions are supported as well. A number of additional endpoints exist for features that are unique to Mitra.

Client API is not stable and may change in minor releases.

[OpenAPI spec](./docs/openapi.yaml)

## Federation

See [FEDERATION.md](./FEDERATION.md)

## ActivityPub Client API

This API is not stable and may be removed in the future.

[Documentation](./docs/c2s.md)

## Development

See [CONTRIBUTING.md](./CONTRIBUTING.md)

### Start database server

```shell
docker-compose up -d
```

Test connection:

```shell
psql -h localhost -p 55432 -U mitra mitra
```

### Start Monero node and wallet server

(this step is optional)

```shell
docker-compose --profile monero up -d
```

### Run web service

Create config file, adjust settings if needed:

```shell
cp config_dev.example.yaml config.yaml
```

Compile and run service:

```shell
cargo run server
```

### Run CLI

```shell
cargo run
```

### Run linter

```shell
cargo clippy
```

### Run tests

```shell
cargo test
```

## License

[AGPL-3.0](./LICENSE)

## Support

Monero: 8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p
