# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Added `VerificationMethod` to `CoreType` enum.
- Added `is_verification_method` function.
- Implemented `PartialEq` for `Hostname` type.
- Added `parse_canonical` method to `Url` type.

### Changed

- Moved `Url` type from `apx_sdk` to `apx_core` package.

### Removed

- Removed `FromStr` implementation from `Url` type.

## [0.12.0] - 2025-04-24

### Added

- Implemented `PartialEq` on `apx_core::ap_url::ApUrl`.
- Implemented `PartialEq` on `apx_sdk::url::Url`.
- Added support for fragment resolution to `fetch_object`.
- Added `DidUrl` type.
- Implemented `parse` method on `VerificationMethod`.
- Implemented `origin` method on `VerificationMethod`.
- Implemented `Display` on `VerificationMethod`.
- Support verification of HTTP signatures created by DID authorities.
- Added `from_multikey` method to `PublicKey` type.

### Changed

- Return `Response` from `send_object` instead of `Option<Response>`.
- Allow choosing backend for `idna`.
- Improved documentation.
- Keep full `DidUrl` when creating `VerificationMethod::DidUrl` enum variant.

### Removed

- Dropped support for `mitra-jcs-rsa-2022` cryptosuite.
- Removed `Did::parse_url` method.

## [0.11.0] - 2025-03-19

### Added

- Added `is_same_http_origin` function.

### Changed

- Allow using `eddsa-jcs-2022` cryptosuite without context injection.
- Added `partOf`, `last`, `next`, `prev` and `current` to the list of collection indicators.

### Fixed

- Fixed validation of portable object fetched from trusted origin.

## [0.10.0] - 2025-03-15

### Added

- Make `with_gateway` function public.
- Added `fep_ef61_trusted_origins` option for `fetch_object`.

### Changed

- Make `get_core_type` return `Collection` if `first` property is present.
- Disallow uppercase letters in HTTP URL host component.
- Compare origins instead of hostnames when verifying fetched non-portable object.
- Moved `encode_hostname` and `guess_protocol` functions to `apx_core::url::hostname` module.

### Removed

- Removed `allow_fep_ef61_noproof` option for `fetch_object`.
- Removed `is_same_hostname` function.

## [0.9.0] - 2025-02-26

### Added

- Support EdDSA HTTP signatures.
- Make `CoreType` and `get_core_type` public.

### Changed

- Include URL in unsafe URL error message.
- Moved `RequestSigner` from `apx_sdk::agent` to `apx_core::http_signatures`.
- Change priority of `Link` in `get_core_type` classifier.
- Renamed `get_object_id` function to `object_to_id`.

### Fixed

- Fixed redirection error when `Location` is a relative URL.

## [0.8.0] - 2025-02-01

### Added

- Added `verification_method_id` method to `DidKey` type.
- Added `sign_object` function.
- Added example of FEP-ae97 server.

### Changed

- Change text representation of deliverer HTTP error to "HTTP error {code}".
- Make `is_actor`, `is_activity`, `is_collection` and `is_object` compatible with FEP-2277.
- Renamed `JsonSigner` type to `VerificationMethod`.
- Renamed `signer` field on `JsonSignatureData` type to `verification_method`.
- Changed type of `message` argument in `create_rsa_sha256_signature` to `&[u8]`.
- Changed return type of `verify_rsa_sha256_signature` to `Result`.
- Changed type of `signature` argument in `verify_eddsa_signature` to `&[u8]`.
- Removed `log` package from dependencies.

## Deprecated

- Marked `sign_object_rsa` as deprecated.

## [0.7.0] - 2024-12-19

### Added

- Implement `Default` for `FederationAgent`.
- Added `remove_quotes` function to `core::http_utils` module.

### Changed

- Make `send_object` return response status and body.
- Make `user_agent` parameter optional in `FederationAgent`.
- Replace `signer_key` and `signer_key_id` fields on `FederationAgent` with `signer` field.
- Make `signer` parameter optional in `FederationAgent`.
- Support media type expression where `profile` parameter is not quoted.

### Removed

- Removed `deliverer_log_response_length` field from `FederationAgent`.
- Removed `is_instance_private` field from `FederationAgent`.

## [0.6.0] - 2024-12-05

### Added

- Added adapters for http version 0.2 types.
- Added `sha256` function to `apx_core::hashes` module.
- Added `sha256_multibase` function to `apx_core::hashes` module.
- Added `deserialize_into_link_href` to `apx_sdk::deserialization` module.

### Changed

- Changed MSRV to 1.66.1.
- Re-export `http` types from `http_types` module.
- Migrated to http package version 1.1.0.
- Migrated to reqwest version 0.12 and rustls version 0.22.
- Use `ContentDigest` type during HTTP signature verification.
- Don't return file size from `fetch_file`.
- Enable HTTP/2 when rustls is used.

## [0.5.0] - 2024-11-20

### Added

- Implement `Debug` and `PartialEq` for `HttpUrl`.
- Added `as_str()` method to `HttpUrl` type.
- Added `origin()` method to `Url` type.
- Implement `Deserialize` for `Url` type.
- Export `iri_string::UriString` as `apx_core::url::common::Uri`.

### Changed

- Rename `JsonSigner` enum variants to `HttpUrl` and `DidUrl`.
- Parse HTTP verification method as `HttpUrl` when verifying JSON signature.
- Parse `keyId` HTTP signature parameter as `HttpUrl`.
- Don't log inbox response if status is not 2xxx.

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
