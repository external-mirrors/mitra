<div align="center">
  <img src="./logo/logo-black-text.svg" alt="Mitra logo" width="150">
</div>

## About

Federated micro-blogging platform.

Built on [ActivityPub](https://www.w3.org/TR/activitypub/) protocol, self-hosted, lightweight. Part of the [Fediverse](https://en.wikipedia.org/wiki/Fediverse).

Features:

- Micro-blogging service
  - Quote posts, custom emojis, reactions and more.
  - Default character limit is 5000.
  - Support for a limited subset of markdown-style formatting.
- Easy installation and small memory footprint (<50 MB).
- Mastodon API.
- Content subscription service. Subscriptions provide a way to receive monthly payments from subscribers and to publish private content made exclusively for them.
  - Supported payment methods: [Monero](https://www.getmonero.org/get-started/what-is-monero/), a peer to peer digital cash system where transactions are private by default.
- Account migrations (from one server to another). Identity can be detached from the server.
- Federation over Tor and/or I2P.

Follow: [@mitra@mitra.social](https://mitra.social/@mitra)

Matrix chat: [#mitra:unredacted.org](https://matrix.to/#/#mitra:unredacted.org)

## Instances

- [Fediverse Observer](https://mitra.fediverse.observer/list)
- [FediList](http://demo.fedilist.com/instance?software=mitra)

Demo instance: https://public.mitra.social/ ([invite-only](https://public.mitra.social/about))

## Supported clients

- [mitra-web](https://codeberg.org/silverpill/mitra-web) (Web)
- [Bloat](https://git.freesoftwareextremist.com/bloat/about/) (Web, NoJS)
- [Phanpy](https://github.com/cheeaun/phanpy) (Web)
- [pl-fe](https://github.com/mkljczk/pl-fe) (Web)
- [Husky](https://github.com/captainepoch/husky) (Android)
- [Fedicat](https://github.com/technicat/fedicat) (iOS)
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
su mitra -s $SHELL -c "mitractl create-account <username> <password> admin"
```

Start Mitra:

```shell
systemctl start mitra
```

An HTTP server will be needed to handle HTTPS requests. See examples of [Nginx](./contrib/mitra.nginx) and [Caddy](./contrib/Caddyfile) configuration files.

### Building from source

Clone the git repository or download the source archive from the [Releases](https://codeberg.org/silverpill/mitra/releases) page.

Install `cargo`. Then run:

```shell
cargo build --release --features production
```

This command will produce two binaries in `target/release` directory, `mitra` and `mitractl`.

Install PostgreSQL, then create the database:

```sql
CREATE USER mitra WITH PASSWORD 'mitra';
CREATE DATABASE mitra OWNER mitra ENCODING 'UTF8';
```

Create configuration file by copying [`config.example.yaml`](./config.example.yaml) and configure the instance. Default config file path is `/etc/mitra/config.yaml`, but it can be changed using `CONFIG_PATH` environment variable.

Put any static files into the directory specified in configuration file. Building instructions for `mitra-web` frontend can be found at https://codeberg.org/silverpill/mitra-web#project-setup.

Create admin account:

```shell
./mitractl create-account <username> <password> admin
```

Start Mitra:

```shell
./mitra
```

An HTTP server will be needed to handle HTTPS requests. See examples of [Nginx](./contrib/mitra.nginx) and [Caddy](./contrib/Caddyfile) configuration files.

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

#### YunoHost

https://apps.yunohost.org/app/mitra

### Hosting providers

- [K&T Host](https://www.knthost.com/mitra)

## Upgrading

Mitra uses semantic versioning (`major.minor.patch`):

- `patch` - bugfixes
- `minor` - improvements and new features
- `major` - breaking changes

Upgrade to a `minor` or a `patch` version is performed by replacing binaries and restarting the service.

Upgrade to a `major` version requires special migration steps that are documented in release notes.

### Debian package

```shell
dpkg -i mitra_amd64.deb
```

Do not overwrite existing configuration file.

## Configuration

### Environment variables

See [defaults](./.env).

### Tor/I2P federation

See [Tor guide](./docs/onion.md) and [I2P guide](./docs/i2p.md).

### Payments

- [Monero](./docs/monero.md)

### IPFS integration (experimental)

See [guide](./docs/ipfs.md).

## Administration

- [Backup and restore](./docs/backup_and_restore.md)
- [Cache management](./docs/cache_management.md)
- [Relays](./docs/relays.md)

## CLI

`mitractl` is a command-line tool for performing instance maintenance.

[Documentation](./docs/mitractl.md)

## Client API

The majority of endpoints imitate Mastodon API. Some Pleroma extensions are supported as well. A number of additional endpoints exist for features that are unique to Mitra.

Client API is not stable and may change in minor releases.

[OpenAPI spec](./docs/openapi.yaml)

## Federation

See [FEDERATION.md](./FEDERATION.md)

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
cargo run
```

### Run CLI

```shell
cargo run --bin mitractl
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
