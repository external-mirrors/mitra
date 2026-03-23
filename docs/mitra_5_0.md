# Mitra 5.0 migration guide

## Preparation

If you're using Mitra 3.x, read the [Mitra 4.0 migration guide](./mitra_4_0.md). Updating from 3.x to 5.x is not supported.

## Dependencies

- Minimum supported version of Rust is 1.85 (this version of the compiler is included in Debian 13).

## Configuration

- Changed the default value of `limits.posts.attachment_local_limit` from `16` to `4`. This is the maximum number of attachments Mastodon can display.
- Removed `federation.fep_c0e0_emoji_react_enabled` configuration parameter.
- Allowed access from all web origins by default.
  - `http_cors_allowlist` configuration parameter enables the old behavior when present.
  - `http_cors_allow_all` configuration parameter was removed.

## Federation

- Removed support for payment links with `https://w3id.org/valueflows/Proposal` rel type.
- Removed support for `MitraJcsRsaSignature2022` proof type.

## CLI

- Removed deprecated `reject-media` and `accept-media` filter actions.
- Removed `read-outbox` command (it can be replaced with `import-object` command).

## HTTP API

- Allowed access from all web origins by default.
- Removed `federated_timeline_restricted` property from `/api/v2/instance` response.
