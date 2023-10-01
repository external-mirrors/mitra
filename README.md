<div align="center">
  <img src="./logo/logo-black-text.svg" alt="Mitra logo" width="150">
</div>

## About

Federated micro-blogging platform.

Built on [ActivityPub](https://www.w3.org/TR/activitypub/) protocol, self-hosted, lightweight. Part of the [Fediverse](https://en.wikipedia.org/wiki/Fediverse).

Features:

- Micro-blogging service (includes support for quote posts, custom emojis and more).
- Mastodon API.
- Content subscription service. Subscriptions provide a way to receive monthly payments from subscribers and to publish private content made exclusively for them.
  - Supported payment methods: [Monero](https://www.getmonero.org/get-started/what-is-monero/) and [ERC-20](https://ethereum.org/en/developers/docs/standards/tokens/erc-20/) tokens (on Ethereum and other EVM-compatible blockchains).
- [Sign-in with a wallet](https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-122.md).
- Donation buttons.
- Account migrations (from one server to another). Identity can be detached from the server.
- Federation over Tor and/or I2P.

Follow: [@mitra@mitra.social](https://mitra.social/@mitra)

Matrix chat: [#mitra:hackliberty.org](https://matrix.to/#/#mitra:hackliberty.org)

## Instances

- [FediList](http://demo.fedilist.com/instance?software=mitra)
- [Fediverse Observer](https://mitra.fediverse.observer/list)

Demo instance: https://public.mitra.social/ ([invite-only](https://public.mitra.social/about))

## Supported clients

- [mitra-web](https://codeberg.org/silverpill/mitra-web) (default)
- [Bloat](https://git.freesoftwareextremist.com/bloat/about/)
- [Husky](https://codeberg.org/husky/husky)
- [toot](https://github.com/ihabunek/toot)

## Code

Server: https://codeberg.org/silverpill/mitra (this repo)

Web client: https://codeberg.org/silverpill/mitra-web

Ethereum contracts: https://codeberg.org/silverpill/mitra-contracts

## Requirements

- PostgreSQL 13+
- Rust 1.63+ (when building from source)

Minimum system requirements:

- 256 MB RAM (1 GB for building from source)
- 10 GB storage for average single user instance with default configuration

Optional integrations:

- Monero wallet service and Monero node
- Ethereum node
- IPFS node (see [guide](./docs/ipfs.md))

## Installation

### Building from source

Run:

```shell
cargo build --release --features production
```

This command will produce two binaries in `target/release` directory, `mitra` and `mitractl`.

Install PostgreSQL and create the database:

```sql
CREATE USER mitra WITH PASSWORD 'mitra';
CREATE DATABASE mitra OWNER mitra;
```

Create configuration file by copying `contrib/mitra_config.yaml` and configure the instance. Default config file path is `/etc/mitra/config.yaml`, but it can be changed using `CONFIG_PATH` environment variable.

Put any static files into the directory specified in configuration file. Building instructions for `mitra-web` frontend can be found at https://codeberg.org/silverpill/mitra-web#project-setup.

Start Mitra:

```shell
./mitra
```

An HTTP server will be needed to handle HTTPS requests. See examples of [Nginx](./contrib/mitra.nginx) and [Caddy](./contrib/Caddyfile) configuration files.

To run Mitra as a systemd service, check out the [systemd unit file example](./contrib/mitra.service).

### Debian package

Download and install Mitra package:

```shell
dpkg -i mitra.deb
```

Install PostgreSQL and create the database:

```sql
CREATE USER mitra WITH PASSWORD 'mitra';
CREATE DATABASE mitra OWNER mitra;
```

Open configuration file `/etc/mitra/config.yaml` and configure the instance.

Start Mitra:

```shell
systemctl start mitra
```

An HTTP server will be needed to handle HTTPS requests. See examples of [Nginx](./contrib/mitra.nginx) and [Caddy](./contrib/Caddyfile) configuration files.

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

## Configuration

### Tor federation

See [guide](./docs/onion.md).

### Blockchain integrations

- [Monero](./docs/monero.md)
- [Ethereum](./docs/ethereum.md)

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
cp config.yaml.example config.yaml
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

## Federation

See [FEDERATION.md](./FEDERATION.md)

## Client API

Most methods are similar to Mastodon API, but Mitra is not fully compatible.

[OpenAPI spec](./docs/openapi.yaml)

## CLI

`mitractl` is a command-line tool for performing instance maintenance.

[Documentation](./docs/mitractl.md)

## License

[AGPL-3.0](./LICENSE)

## Support

Monero: 8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p
