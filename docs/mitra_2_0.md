# Mitra 2.0 migration guide

## Dependencies

- Minimum supported version of PostgreSQL is 13. Version 12 may work too, but upgrading to a newer version is recommended.
- Recommended version of Rust is 1.63 (this is the version of `rustc` package in Debian 12 Bookworm). Mitra 2.0 can be compiled using Rust 1.57, but upgrading to a newer version of the compiler is recommended.
- Minimum supported version of Node.js is 16.

## Configuration

- Removed `registrations_open` configuration parameter. Use `registration.type` instead.
- Removed `registration.default_role_read_only_user` configuration parameter. Use `registration.default_role` with value `read_only_user` instead.
- Removed `post_character_limit` configuration parameter. Use `limits.posts.character_limit` instead.
- Removed `proxy_url` configuration parameter. Use `federation.proxy_url` instead.
- Changed the default value of `authentication_methods` configuration parameter to `["password"]`. "Sign in with Ethereum" is now disabled by default.
- Changed the default value of `instance_staff_public` configuration parameter to `true`.
- Changed the default value of `limits.posts.character_limit` configuration parameter to `5000`.

Prior to version 2.0, Mitra printed warnings when it encountered deprecated configuration parameters. In Mitra 2.0 these parameters are ignored.

See [annotated example of config.yaml file](../contrib/mitra_config.yaml) for more information.

## HTTP API

- `X-Forwarded-Proto` header must be present when Mitra runs behind a reverse proxy. Instance operators who use Nginx should add `proxy_set_header X-Forwarded-Proto $scheme;` directive to a relevant `location` block. See [example of nginx config file](../contrib/mitra.nginx). Caddy sets `X-Forwarded-Proto` header by default, so no action is needed.
- Enabled redirection from `/feeds/{username}` to `/feeds/users/{username}`. Old Atom feed paths are considered deprecated and will be removed in the future.
