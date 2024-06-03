# Contributing

## General

Mitra is developed according to these principles:

- Resilience. The primary function of Mitra is delivery of messages from publisher to the audience. It should be able to perform this task even in adversarial conditions.
- Self-hosting. If some feature depends on other service (such as blockchain node), that service must be free / open source software and it must be able to run on affordable hardware. No dependecies on proprietary services allowed.
- Low system requirements. The default configuration should work smoothly on a low-end VPS.
- Easy to operate. Installation and maintenance should be as simple as possible. Moderation tasks should be delegated to users.
- Privacy. In its default configuration, Mitra shouldn't require any personal info (other than username / public key) or collect usage statistics. It also shouldn't reveal more information about the user than necessary.

## Before you start

If you want to propose a change, please create an [issue](https://codeberg.org/silverpill/mitra/issues) first and explain what you want to do (unless it's something trivial).

## Code

Simplicity is more important than minor performance improvements.

Avoid advanced language features unless there's a good reason to use them. The code should be comprehensible even to a Rust beginner.

### MSRV

The MSRV must not be greater than the version of [rustc package](https://tracker.debian.org/pkg/rustc) in Debian testing.

### Dependencies

Try to minimize the number of dependencies.

Prefer libraries maintained by volunteers over those developed by for-profit companies.

### Code style

Run `cargo clippy` to check code automatically.

- Try to follow the existing style when adding new features.
- Use `expect()` to check invariants. Other errors should be handled.

### Commits

Commits should be atomic (the tests should pass) and not too big. Commit messages should be informative.

For any notable change there should be an entry in [CHANGELOG.md](./CHANGELOG.md).
