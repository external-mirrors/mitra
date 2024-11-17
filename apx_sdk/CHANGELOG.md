# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Implement `Debug` and `PartialEq` for `HttpUrl`.
- Added `as_str()` method to `HttpUrl` type.
- Added `origin()` method to `Url` type.

### Changed

- Rename `JsonSigner` enum variants to `HttpUrl` and `DidUrl`.
- Parse HTTP verification method as `HttpUrl` when verifying JSON signature.
- Parse `keyId` HTTP signature parameter as `HttpUrl`.

## [0.4.0] - 2024-11-07

### Added

- Added `skip_verification` parameter to `fetch_object` options.

### Changed

- Make `fetch_object` return `JsonValue`.
- Make `fetch_json` return `JsonValue`.
- Pass `FetchObjectOptions` type to `fetch_object`.

## [0.3.0] - 2024-10-16

### Added

- Added `test-utils` feature to `apx_sdk` that enables `test-utils` feature on `apx_core`.
- Added `hostname()` method to `HttpUrl` type.

## [0.2.0] - 2024-10-03

### Added

- Added `is_collection` function to `utils` module.
- Re-export `apx_core` in `apx_sdk`.
- Re-export `http` in `apx_core::http_signatures`.

### Changed

- Allow `proof.verificationMethod` to be DID URL.
- Don't duck-type collections as "objects".
- Move `url_encode` and `url_decode` functions to `apx_core::url::common`.
- Accept 'ap' URLs with percent-encoded authority.
- Change `parse_http_signature` to accept `HeaderMap`.
- Changed MSRV to 1.65.0.

### Fixed

- Fixed incorrect reporting of object ID errors during fetching.
- Fixed incorrect processing of "created" value during FEP-8b32 proof verification.

## [0.1.0] - 2024-09-17

### Added

- Initial release.
