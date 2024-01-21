# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added

- Add syndication feed link to user's Webfinger response.

### Changed

- Ignore Webfinger links where `type` differs from AP/AS media types.

## [2.7.2] - 2024-01-18

### Added

- Add support for rustls to `mitra_federation` package.

### Changed

- Add actor ID to unsolicited message report.
- Don't strip query during key ID processing if `id` query parameter is present.

## [2.7.1] - 2024-01-13

### Changed

- Validate OAuth redirect URIs.
- Generate random nonce for inline styles on authorization page.

### Fixed

- Fix authorization form redirect.

## [2.7.0] - 2024-01-12

### Added

- Support media uploads in `multipart/form-data` format.
- Added `/api/v2/media` Mastodon API endpoint.
- Added `/api/v1/media/{attachment_id}` API endpoint.

### Changed

- Hide reposts from public timelines.
- Validate media descriptions before saving to database.
- Fetch replies collection even if it is embedded.
- Write warning to log when `Digest` header is not present on signed request.
- Ensure mentions exist for all local actors in "to" and "cc" fields.
- Change certificate store log message level from warning to error.

### Removed

- Removed redirects on `/profile/{profile_id}` routes.

### Fixed

- Fill `in_reply_to_account_id` value on `Status.reblog` object.
- Fixed extraction of media IDs from urlencoded form data.
- Prevent pruning of quoted posts.

## [2.6.0] - 2023-12-25

### Added

- Allow updating preferred username with `refetch-actor` command.
- Add `audio/flac` to the list of supported media types.
- Added `actor_id` property to Mastodon API `Account` object.
- Added support for `--version` flag to `mitractl`.
- Added `in_reply_to_account_id` property to Mastodon API `Status` object.
- Automatically delete unused images after profile updates.
- Support media descriptions.

### Changed

- Allow calling `/api/v1/accounts/{account_id}/follow` with an empty body.
- Allow to call `delete-profile` command with username as first argument.
- Change audience expansion error message.
- Write a message to log if SSL certificate probe was not successful.
- Allow `actor` property of `Follow`, `Accept` and `Delete` activities to contain object.
- Changed `mitractl` help message (provided by Rust `clap` library).
- Treat `jcs-eddsa-2022` and `eddsa-jcs-2022` cryptosuites as different.
- Use `jcs-eddsa-2022` cryptosuite for signing activities.
- Improved object type disambiguation in `Update()` activity handler.
- Process all objects with `attributedTo` property as notes.
- Change title of Atom feed to profile's display name.
- Perform media cleanup in scheduled task instead of using `tokio::spawn`.
- Check hashtag length before saving to database.

### Fixed

- Ignore deleted recipients when processing outgoing activity queue.
- Fixed key ID to actor ID translation when key ID contains query string.

## [2.5.0] - 2023-12-14

### Added

- Support changing logger configuration with `RUST_LOG` environment variable.
- Make length of logged delivery response configurable.

### Changed

- Allow `assertionMethod` property value to be object.
- Attempt to load additional page when importing replies from Mastodon.
- Change instance actor type from `Service` to `Application`.
- Write delivery description to log on every attempt.
- Don't log activities from blocked instances.

### Fixed

- Fix delivery of `Delete(Person)` activities.

## [2.4.1] - 2023-11-23

### Added

- Add `url` field to Mastodon API Status object.

### Changed

- Write instance URL to log at startup.
- Change MSRV to 1.60.0.
- Allow cross-origin requests from 127.0.0.1 by default.

## [2.4.0] - 2023-11-16

### Added

- Support allowlist federation.
- Add `limits.media.extra_supported_types` configuration option.
- Add `fetch_object_as` command.
- Parse FEP-1970 chat links attached to actor objects.

### Changed

- Allow to use wildcard pattern in `blocked_instances` list.
- Prefer value of Content-Type header when determining media type of a fetched file.
- Don't accept remote profile images with media type prefix other than `image/`.
- Ignore `Announce(Block)` activities from Lemmy.
- Disallow relative links in posts.
- Don't log actor ID redirection events.
- Replace inline images with "image" string instead of removing completely.
- Change MSRV to 1.59.0.

### Fixed

- Fix feature list for chrono crate in mitra-utils.

## [2.3.0] - 2023-11-07

### Added

- Add `created_at` field to response of `/api/v1/subscriptions/invoices` endpoint.
- Add "replies" collection to `Note` objects.
- Support fetching replies from Mitra instances.
- Add `authentication_token_lifetime` configuration parameter.

### Changed

- Disable email autolinking in bio and key-value fields.
- Don't convert fetched objects to strings before deserialization.
- Make "role" argument optional for `create-user` command.

### Fixed

- Always add `rel=noopener` to links.
- Remove context from `Note` objects in featured collection.
- Fix `list-users` command not displaying users who never logged in.
- Return correct `authentication_methods` list after setting password with `/api/v1/settings/change_password`.

### Security

- Limit response size when making federation requests.

## [2.2.0] - 2023-10-22

### Added

- Display number of generated Monero addresses in instance report.
- Allow posts with attachment and no text.
- Add `updated` property to edited `Note` objects.
- Implement `/api/v1/statuses/{status_id}/source` API endpoint.
- Add API endpoint for updating posts.

### Changed

- Do not show outgoing subscriptions in instance report.
- Set limit on a number of hashtags in a post.
- Change default content type of a post to `text/markdown`.
- Stop logging incoming followers-only posts.
- Require local direct messages to have at least one mention.

### Removed

- Drop support for `mentions` parameter in `/api/v1/statuses` form data.

### Fixed

- Fix clearing of attachments during post update.

## [2.1.0] - 2023-10-10

### Added

- Add rate-limited variant of `/api/v1/accounts/search` endpoint.

### Changed

- Allow `gemini:` and `monero:` links in post content.
- Update `monerod` and `monero-wallet-rpc` containers.
- Check chain ID consistency when reading/writing payment options.
- Redirect to client if proposal chain ID is in Ethereum namespace.
- Use proposal ID as target in Ethereum subscription payment links.
- Replace HTTP signature expiration warning with error.

### Fixed

- Fix alias list overwrite during profile update.
- Require alias to be different account.
- Don't discard query and fragment when parsing local ActivityPub IDs.

## [2.0.0] - 2023-10-02

### Changed

- Change default value of `authentication_methods` configuration parameter to `["password"]`.
- Change default value of `instance_staff_public` configuration parameter to `true`.
- Change default value of `limits.posts.character_limit` to 5000.
- Redirect from `/feeds/{username}` to `/feeds/users/{username}`.

### Removed

- Remove `registrations_open` configuration parameter.
- Remove `registration.default_role_read_only_user` configuration parameter.
- Remove `post_character_limit` configuration parameter.
- Remove `proxy_url` configuration parameter.
- Disable protocol guessing when determining base URL.
- Remove support for `JcsRsaSignature2022` and `JcsEip191Signature2022` proof types.
- Don't read `current_block` file.

## [1.36.2] - 2023-09-30

### Added

- Support `x86_64-unknown-linux-musl` build target.

## [1.36.1] - 2023-09-28

### Added

- Support `image/avif` media type.
- Added `allow_unauthenticated.timeline_local` parameter to `Instance` object.

### Changed

- Accept forwarded `Update(Note)` activities.

### Removed

- Remove `/api/v1/accounts/send_activity` API endpoint (replaced by outbox).

## [1.36.0] - 2023-09-19

### Added

- Allow creating invoices for remote recipients.
- Send `Offer(Agreement)` activity to start payment process.
- Implement handler for `Offer(Agreement)` activities.
- Implement handler for `Accept(Offer)` activities.
- Track status of remote subscriptions.

### Changed

- Use `resourceConformsTo` property of proposal object to determine support for FEP-0837.
- Don't change invoice status to "Forwarded" if payout tx ID is not set.

## [1.35.0] - 2023-09-11

### Added

- Added outbox POST handler (FEP-ae97).
- Added OpenGraph renderer for Synapse link preview generator.
- Added `instance_timeline_public` configuration parameter for changing visibility of local timeline.
- Add reference to agreement object to `Add(Person)` activity.

### Changed

- Update `/api/v1/accounts/send_activity` to accept only signed activities.
- Support calling `/api/v1/accounts/relationships` endpoint with multiple IDs.
- Verify subscription option with correct chain ID exists before creating invoice.
- Support fetching outboxes with embedded first page.
- Updated Caddyfile example ([#48](https://codeberg.org/silverpill/mitra/pulls/48)).
- Copy terms from default JSON-LD context into proposal context.

### Removed

- Remove `params` field from `/api/v1/accounts/signed_update` response.

### Fixed

- Remove context from object of `Create(Note)` activity.
- Remove context from object of `Undo(Follow)` activity.

## [1.34.0] - 2023-08-31

### Added

- Add list of staff accounts to NodeInfo metadata object.
- Add creation date to relationship and follow request database records.
- Implement `/api/v1/follow_requests` API endpoint.
- Implement `/api/v1/follow_requests/{account_id}/authorize` and `/api/v1/follow_requests/{account_id}/reject` API endpoints.
- Support manual approval of followers.
- Add `rejected_by` attribute to Mastodon API Relationship object.
- Append links from `Link` attachments to post content.
- Add subscription expiration time to `Add(Person)` activity.

### Changed

- Don't create follow request if follow relationship exists.
- Create rejection relationship instead of setting "Rejected" status on follow request.
- Unfollow actor if it sends `Reject(Follow)` activity.

### Fixed

- Fix incorrect relationship records in database.

## [1.33.1] - 2023-08-20

### Changed

- Remove trailing slashes from requests' paths.

### Fixed

- Put empty string into `spoiler_text` attribute of `Status` object instead of null.

## [1.33.0] - 2023-08-17

### Added

- Enable previews of remote subscription options.
- Verify integrity proof on activities submitted using `/api/v1/accounts/send_activity` API endpoint.

### Changed

- Hide reposts of muted accounts.
- Make `params` optional for `/api/v1/accounts/send_activity` API endpoint.
- Use `eddsa-jcs-2022` cryptosuite instead of `jcs-eddsa-2022` for FEP-8b32 proofs.
- Prevent activity deliveries from blocking each other.
- Accept `Announce(Delete(Tombstone))` activities.

### Removed

- Remove support for client-side activity signing using Minisign.

### Fixed

- Prevent HTML cleaner from removing `rel=tag`.

### Security

- Improve validation of remote media URLs.

## [1.32.0] - 2023-08-10

### Added

- Delete repost when receiving `Announce(Delete)` activity from a group.
- Deliver activities to multiple inboxes in parallel.
- Added admin account info to `/api/v1/instance` response.
- Added "role" column to `list-users` command output.
- Added `mediaType` property to proposal FEP-0ea0 link.
- Support `eddsa-jcs-2022` cryptosuite (alias of `jcs-eddsa-2022`).

### Changed

- Don't retry delivery if recipient had prior unreachable status.
- Don't re-create activity when processing user-signed `Update()` activity.
- Ignore `Announce(Lock)` activities from Lemmy.
- Measure activity delivery time.
- Make `fetch-replies` command work with Akkoma.
- Improve logging of invoice processing.
- Ensure subscription price is always non-zero.
- Accept `Update(Group)` activities.
- Change `/oauth/revoke` API endpoint to return empty object.
- Add `rel=tag` attribute to hashtags.

### Removed

- Remove `check-expired-invoice` alias of `reopen-invoice` command.
- Remove `message` field from Mastodon API error response.
- Remove `post_character_limit` field from `/api/v1/instance` response.

### Fixed

- Added missing `Hashtag` type to object context.
- Use correct request timeout during HTTP signature verification.
- Fixed documentation of `authentication_methods` configuration parameter.

## [1.31.1] - 2023-07-30

### Added

- Show total number of posts in instance report.

### Changed

- Allow to import objects with type `Proposal` as posts.
- Add `to` property to proposals.

### Fixed

- Make `fetch-replies` command not panic if replies collection is not present.
- Make `fetch-replies` command not panic if replies collection doesn't contain items.
- Optimize database query used in `DeleteExtraneousPosts` task.

## [1.31.0] - 2023-07-26

### Added

- Support `jcs-eddsa-2022` identity proofs.
- Fetch and parse proposals attached to remote actors.
- Add activity queue stats to `instance-report` command output.
- Allow to filter profile timeline with `only_media` parameter.
- Support non-ascii hashtags.

### Changed

- Write total number of objects to log when fetching replies.
- Changed license ID to `AGPL-3.0-only`.
- Add `attributedTo` property to proposals.
- Fetch `replies` collection when it is not embedded.
- Make `read-outbox` command work with non-paginated collections.
- Return error when trying to save local profile with payment links.
- Add `chain_id` parameter to payment options on `Account` object.
- Perform canonicalization on the client side when signing `Update()` activity.
- Write message to log when encountering invalid hashtag.

### Fixed

- Fix for compatibility with Alpine Linux.
- Fix deserialization of `rel` arrays.
- Use correct CAIP-19 asset type for Monero testnets and Wownero.

## [1.30.0] - 2023-07-18

### Added

- Use standardized CAIP-2 Monero chain identifiers.
- Generate Valueflows `Proposal` objects for Monero subscriptions.
- Allow actor's Ed25519 key to be used for assertions.
- Add `fetch-replies` command.

### Changed

- Require `chain_id` parameter for registering subscription option.
- Import posts from outbox in chronological order.
- Add `created_at` parameter to identity claim API response.
- Replace URL in subscription payment link with a corresponding proposal ID.
- Add media type to upload type validation error message.
- Set default visibility of a post to direct when parent post is not public.
- Convert "unlisted" visibility parameter to "public".
- Fetch object of `Update()` activity if it is not embedded.

### Fixed

- Fix parsing of FEP-c390 attachments.
- Fix ordering of profile timeline.

## [1.29.0] - 2023-07-05

### Added

- Add `list-users` command.

### Changed

- Update identity proof validation API to use `proof_type` parameter.
- Ignore `Announce(Add)` and `Announce(Remove)` activities from Lemmy.
- Check uniqueness of issuers when saving identity proofs.
- Check uniqueness of chain IDs when saving payment options.
- Accept integrity proofs with `authentication` purpose.
- Allow to call `set-password` and `set-role` commands with username as first argument.
- Reset reachability status when remote profile is updated.

### Fixed

- Remove `<img>` tags instead of clearing `src` attribute.

## [1.28.1] - 2023-06-27

### Fixed

- Added workaround for Pleroma collection parsing bug.

## [1.28.0] - 2023-06-26

### Added

- Support FEP-c390 identity proofs.
- Allow to pin posts to profile.
- Added `instance-report` command.

### Changed

- Accept minisign public key and signature files for identity proof generation.
- Verify actor doesn't have duplicate public keys.
- Reject disconnected replies if author doesn't have local followers.
- Return error if CAIP-2 namespace is not `eip155` or `monero`.
- Improve validation of Monero chain IDs.

## [1.27.0] - 2023-06-14

### Added

- Added `add-emoji` command.
- Create representation of actor's RSA public key in multikey format.
- Use multikeys for actor authentication.
- Support Ed25519 actor keys.
- Implement FEP-8b32 with `jcs-eddsa-2022` cryptosuite.
- Support Mastodon's follow export format.

### Changed

- Allow to replace imported custom emojis.
- Handle activities where "actor" property contains an object.
- Sniff media type if declared type of downloaded file is application/octet-stream.
- Return `404 Not Found` if inbox owner doesn't exist.
- Refresh outdated actor profiles when doing actor address lookups.
- Set signature verification fetcher timeout to 10 seconds.
- Make account search work when instance name is incomplete.
- Added `manifest-src` directive to CSP header.
- Stop logging skipped actor tags.

### Fixed

- Allow "url" property to contain list of strings.
- Fix emoji regexp in microsyntax parser.

## [1.26.0] - 2023-06-04

### Added

- Support federation with Bridgy Fed.
- Support federation with Mobilizon.
- Store IDs of payout transactions in database.
- Update subscription only when payout transaction is confirmed.
- Re-open closed invoices when address receives new payment.
- Add `get-payment-address` command.

### Changed

- Use ActivityPub format when saving posts to IPFS.
- Added `video/quicktime` to the list of supported media types.
- Disallow media uploads without media type.
- Make `chain_id` parameter required at `/api/v1/subscriptions/invoices`.
- Change `/api/v1/timelines/public` to return federated timeline by default.
- Reject direct messages without mentions.
- Improved recovery of failed payout transactions.
- Accept Update(Article) activities.
- Don't save remote file if media type is not supported.
- Use `Content-Type` header to assign media type to file.

### Fixed

- Fixed panic in `import_post` when trying to import local object.
- Ignore address with index 0 when looking for missed payments.
- Tolerate account index mismatch when it is caused by configuration change.
- Allow unquoted HTTP signature parameters.

## [1.25.0] - 2023-05-25

### Added

- Make `/api/v1/timelines/public` return public timeline if `local` is set to `false`.
- Add `/api/v1/timelines/direct` API endpoint.
- Added full list of declared aliases to `/api/v1/accounts/{account_id}/aliases/all` response.
- Created API endpoint for removing aliases.
- Display authorization code if OAuth `redirect_uri` equals `urn:ietf:wg:oauth:2.0:oob`.
- Implement `jcs-eddsa-2022` cryptosuite.
- Enabled parsing of FEP-fb2a actor metadata fields.
- Allow to specify chain ID for invoice.
- Added `activeHalfyear` and `activeMonth` metrics to NodeInfo.
- Added `list-active-addresses` command.

### Changed

- Return validation error if trying to add alias that already exists.
- Make `/api/v1/apps` and `/oauth/token` endpoints accept `multipart/form-data`.
- Don't retry incoming activity if fetcher encounters `404 Not Found` error.
- Rename `check-expired-invoice` command to `reopen-invoice` and allow to reopen invoices with "forwarded" status.
- Allow to call `reopen-invoice` command with payment address as an argument.
- Verify that account index returned by monero-wallet-rpc matches configuration.
- Change invoice status to "underpaid" if amount is too small to be forwarded.
- Append attachment URL to post if attachment can't be downloaded.

### Deprecated

- Loading local timeline without `local` parameter.
- Creating invoice without specifying `chain_id` parameter.

### Removed

- Removed `/api/v1/accounts/aliases` API endpoint.

## [1.24.1] - 2023-05-15

### Added

- Added `include_expired` parameter to `/api/v1/accounts/{account_id}/subscribers`.

## [1.24.0] - 2023-05-14

### Added

- Added pagination header to Timeline API responses.
- Use `name` and `summary` attributes to create post title.
- Added `preview_url` attribute to attachments in Mastodon API.
- Added API endpoint for cancelling invoices.
- Added optional `chain_metadata.description` field to Monero blockchain config.

### Changed

- Improve validation of FEP-0ea0 payment links.
- Added `video/x-m4v` to supported media types.

### Fixed

- Fix wrong hostname in pagination header.
- Preserve query parameters when creating pagination header.
- Return validation error if follow or mute target is current user.

## [1.23.0] - 2023-05-03

### Added

- Add `federation.fep_e232_enabled` configuration parameter.
- Make authentication methods configurable.
- Save post source if it is authored in markdown.
- Validate monero chain ID when reading configuration file.
- Support managed database connections with TLS ([#34](https://codeberg.org/silverpill/mitra/pulls/34)).
- Prevent re-use of EIP-4361 nonces.
- Added `create-monero-signature` and `verify-monero-signature` commands.
- Support "Sign In With Monero" (CAIP-122).
- Allow muting and unmuting users ([#35](https://codeberg.org/silverpill/mitra/pulls/35)).

### Changed

- Set default `authentication_method` to `password` for `/api/v1/accounts` endpoint.
- Allow EIP-4361 messages to have expiration time.
- Keep `state` parameter when redirecting from OAuth authorization page.
- Change default limit of `read-outbox` command to 20 activities.
- Change maximum length of local usernames to 30.
- Update CAIP-10 account address regexp.
- Present first object link in a post as a Misskey quote.

### Removed

- Remove `fep-e232` cargo feature.
- Drop support for `ethereum` OAuth flow.

### Fixed

- Fix JSON-LD context of `Note` objects.

## [1.22.0] - 2023-04-22

### Added

- Added support for content warnings.
- Added `authentication_methods` field to `CredentialAccount` object (Mastodon API).
- Support integrity proofs with `DataIntegrityProof` type.
- Add `federation.i2p_proxy_url` configuration parameter.

### Changed

- Ignore errors when importing activities from outbox.
- Make activity limit in outbox fetcher adjustable.
- Changed `reset-subscriptions` command arguments (removes subscription options by default).
- Return error if specified Monero account doesn't exist.
- Updated actix to latest version. MSRV changed to 1.57.
- Make `/api/v1/accounts` endpoint accept optional `authentication_method` parameter.
- Make attached subscription links compatible with FEP-0ea0.
- Add replies and reposts to outbox collection.

### Deprecated

- Calling `/api/v1/accounts` without `authentication_method` parameter.

### Fixed

- Make `/api/v1/accounts/{account_id}/follow` work with form-data.
- Make `onion_proxy_url` override `proxy_url` setting if request target is onion.

## [1.21.0] - 2023-04-12

### Added

- Support Monero Wallet RPC authentication.
- Added `create-user` command.
- Added `read-outbox` command.

### Changed

- Added emoji count check to profile data validator.
- Check mention and link counts when creating post.
- Disable transaction monitor tasks if blockchain integration is disabled.
- Allow multiple configurations in `blockchains` array.
- Re-fetch object if `attributedTo` value doesn't match `actor` of `Create` activity.
- Added actor validation to `Update(Note)` and `Undo(Follow)` handlers.

### Fixed

- Fixed database query error in `Create` activity handler.

## [1.20.0] - 2023-04-07

### Added

- Support calling `/api/v1/accounts/search` with `resolve` parameter.
- Created `/api/v1/accounts/aliases/all` API endpoint.
- Created API endpoint for adding aliases.
- Populate `alsoKnownAs` property on actor object with declared aliases.
- Support account migration from Mastodon.
- Created API endpoint for managing client configurations.
- Reject unsolicited public posts.

### Changed

- Increase maximum number of custom emojis per post to 50.
- Validate actor aliases before saving into database.
- Process incoming `Move()` activities in background.
- Allow custom emojis with `image/webp` media type.
- Increase object ID size limit to 2000 chars.
- Increase fetcher timeout to 15 seconds when processing search queries.

### Fixed

- Added missing `CHECK` constraints to database tables.
- Validate object ID length before saving post to database.
- Validate emoji name length before saving to database.

## [1.19.1] - 2023-03-31

### Changed

- Limit number of mentions and links in remote posts.

### Fixed

- Process queued background jobs before re-trying stalled.
- Remove activity from queue if handler times out.
- Order attachments by creation date when new post is created.

## [1.19.0] - 2023-03-30

### Added

- Added `prune-remote-emojis` command.
- Prune remote emojis in background.
- Added `limits.media.emoji_size_limit` configuration parameter.
- Added `federation.fetcher_timeout` and `federation.deliverer_timeout` configuration parameters.

### Changed

- Allow emoji names containing hyphens.
- Increased remote emoji size limit to 500 kB.
- Set fetcher timeout to 5 seconds when processing search queries.

### Fixed

- Fixed error in emoji update SQL query.
- Restart stalled background jobs.
- Order attachments by creation date.
- Don't reopen monero wallet on each subscription monitor run.

### Security

- Updated markdown parser to latest version.

## [1.18.0] - 2023-03-21

### Added

- Added `fep-e232` feature flag (disabled by default).
- Added `account_index` parameter to Monero configuration.
- Added `/api/v1/instance/peers` API endpoint.
- Added `federation.enabled` configuration parameter that can be used to disable federation.

### Changed

- Documented valid role names for `set-role` command.
- Granted `delete_any_post` and `delete_any_profile` permissions to admin role.
- Updated profile page URL template to match mitra-web.

### Fixed

- Make webclient-to-object redirects work for remote profiles and posts.
- Added webclient redirection rule for `/@username` routes.
- Don't allow migration if user doesn't have identity proofs.

## [1.17.0] - 2023-03-15

### Added

- Enabled audio and video uploads.
- Added `audio/ogg` and `audio/x-wav` to the list of supported media types.

### Changed

- Save latest ethereum block number to database instead of file.
- Removed hardcoded upload size limit.

### Deprecated

- Reading ethereum block number from `current_block` file.

### Removed

- Disabled post tokenization (can be re-enabled with `ethereum-extras` feature).
- Removed ability to switch from Ethereum devnet to another chain without resetting subscriptions.

### Fixed

- Allow `!` after hashtags and mentions.
- Ignore emojis with non-unique names in remote posts.

## [1.16.0] - 2023-03-08

### Added

- Allow to add notes to generated invite codes.
- Added `registration.default_role` configuration option.
- Save emojis attached to actor objects.
- Added `emojis` field to Mastodon API Account entity.
- Support audio attachments.
- Added CLI command for viewing unreachable actors.
- Implemented NodeInfo 2.1.
- Added `federation.onion_proxy_url` configuration parameter (enables proxy for requests to `.onion` domains).

### Changed

- Use .jpg extension for files with image/jpeg media type.

### Deprecated

- Deprecated `default_role_read_only_user` configuration option (replaced by `registration.default_role`).

## [1.15.0] - 2023-02-27

### Added

- Set fetcher timeout to 3 minutes.
- Set deliverer timeout to 30 seconds.
- Added `federation` parameter group to configuration.
- Add empty `spoiler_text` property to Mastodon API Status object.
- Added `error` and `error_description` fields to Mastodon API error responses.
- Store information about failed activity deliveries in database.
- Added `/api/v1/accounts/{account_id}/aliases` API endpoint.

### Changed

- Put activities generated by CLI commands in a queue instead of immediately sending them.
- Changed path of user's Atom feed to `/feeds/users/{username}`.
- Increase number of delivery attempts and increase intervals between them.

### Deprecated

- Deprecated `proxy_url` configuration parameter (replaced by `federation.proxy_url`).
- Deprecated Atom feeds at `/feeds/{username}`.
- Deprecated `message` field in Mastodon API error response.

### Fixed

- Prevent `delete-extraneous-posts` command from removing locally-linked posts.
- Make webfinger response compatible with GNU Social account lookup.
- Prefer `Group` actor when doing webfinger query on Lemmy server.
- Fetch missing profiles before doing follower migration.
- Follow FEP-e232 links when importing post.

## [1.14.0] - 2023-02-22

### Added

- Added `/api/v1/apps` endpoint.
- Added OAuth authorization page.
- Support `authorization_code` OAuth grant type.
- Documented `http_cors_allowlist` configuration parameter.
- Added `/api/v1/statuses/{status_id}/thread` API endpoint (replaces `/api/v1/statuses/{status_id}/context`).
- Accept webfinger requests where `resource` is instance actor ID.
- Added `proxy_set_header X-Forwarded-Proto $scheme;` directive to nginx config example.
- Add `Content-Security-Policy` and `X-Content-Type-Options` headers to all responses.

### Changed

- Allow `instance_uri` configuration value to contain URI scheme.
- Changed `/api/v1/statuses/{status_id}/context` response format to match Mastodon API.
- Changed status code of `/api/v1/statuses` response to 200 to match Mastodon API.
- Removed `add_header` directives for `Content-Security-Policy` and `X-Content-Type-Options` headers from nginx config example.

### Deprecated

- Deprecated protocol guessing on incoming requests (use `X-Forwarded-Proto` header).

### Fixed

- Fixed actor object JSON-LD validation errors.
- Fixed activity JSON-LD validation errors.
- Make media URLs in Mastodon API responses relative to current origin.

## [1.13.1] - 2023-02-09

### Fixed

- Fixed permission error on subscription settings update.

## [1.13.0] - 2023-02-06

### Added

- Replace post attachments and other related objects when processing `Update(Note)` activity.
- Append attachment URL to post content if attachment size exceeds limit.
- Added `/api/v1/custom_emojis` endpoint.
- Added `limits` parameter group to configuration.
- Made file size limit adjustable with `limits.media.file_size_limit` configuration option.
- Added `limits.posts.character_limit` configuration parameter (replaces `post_character_limit`).
- Implemented automatic pruning of remote posts and empty profiles (disabled by default).

### Changed

- Use proof suites with prefix `Mitra`.
- Added `https://w3id.org/security/data-integrity/v1` to JSON-LD context.
- Return `202 Accepted` when activity is accepted by inbox endpoint.
- Ignore forwarded `Like` activities.
- Set 10 minute timeout on background job that processes incoming activities.
- Use "warn" log level for delivery errors.
- Don't allow read-only users to manage subscriptions.

### Deprecated

- Deprecated `post_character_limit` configuration option.

### Fixed

- Change max body size in nginx example config to match app limit.
- Don't create invoice if recipient can't accept subscription payments.
- Ignore `Announce(Delete)` activities.

## [1.12.0] - 2023-01-26

### Added

- Added `approval_required` and `invites_enabled` flags to `/api/v1/instance` endpoint response.
- Added `registration.type` configuration option (replaces `registrations_open`).
- Implemented roles & permissions.
- Added "read-only user" role.
- Added configuration option for automatic assigning of "read-only user" role after registration.
- Added `set-role` command.

### Changed

- Don't retry activity if fetcher recursion limit has been reached.

### Deprecated

- `registrations_open` configuration option.

### Removed

- Dropped support for `blockchain` configuration parameter.

### Fixed

- Added missing `<link rel="self">` element to Atom feeds.
- Added missing `<link rel="alternate">` element to Atom feed entries.

## [1.11.0] - 2023-01-23

### Added

- Save sizes of media attachments and other files to database.
- Added `import-emoji` command.
- Added support for emoji shortcodes.
- Allowed custom emojis with `image/apng` media type.

### Changed

- Make `delete-emoji` command accept emoji name and hostname instead of ID.
- Replaced client-side tag URLs with collection IDs.

### Security

- Validate emoji name before saving.

## [1.10.0] - 2023-01-18

### Added

- Added `/api/v1/settings/move_followers` API endpoint (replaces `/api/v1/accounts/move_followers`).
- Added `/api/v1/settings/import_follows` API endpoint.
- Validation of Monero subscription payout address.
- Accept webfinger requests where `resource` is actor ID.
- Adeed support for `as:Public` and `Public` audience identifiers.
- Displaying custom emojis.

### Changed

- Save downloaded media as "unknown" if its media type is not supported.
- Use `mediaType` property value to determine file extension when saving downloaded media.
- Added `mediaType` property to images in actor object.
- Prevent `delete-extraneous-posts` command from deleting post if there's a recent reply or repost.
- Changed max actor image size to 5 MB.

### Removed

- `/api/v1/accounts/move_followers` API endpoint.

### Fixed

- Don't ignore `Delete(Person)` verification errors if database error subtype is not `NotFound`.
- Don't stop activity processing on invalid local mentions.
- Accept actor objects where `attachment` property value is not an array.
- Don't download HTML pages attached by GNU Social.
- Ignore `Like()` activity if local post doesn't exist.
- Fixed `.well-known` paths returning `400 Bad Request` errors.

## [1.9.0] - 2023-01-08

### Added

- Added `/api/v1/accounts/lookup` Mastodon API endpoint.
- Implemented activity delivery queue.
- Started to keep track of unreachable actors.
- Added `configuration` object to response of `/api/v1/instance` endpoint.
- Save media types of uploaded avatar and banner images.
- Support for `MitraJcsRsaSignature2022` and `MitraJcsEip191Signature2022` signature suites.

### Changed

- Updated installation instructions, default mitra config and recommended nginx config.
- Limited the number of requests made during the processing of a thread.
- Limited the number of media files that can be attached to a post.

### Deprecated

- Deprecated `post_character_limit` property in `/api/v1/instance` response.
- Avatar and banner uploads without media type via `/api/v1/accounts/update_credentials`.
- `JcsRsaSignature2022` and `JcsEip191Signature2022` signature suites.

### Removed

- Removed ability to upload non-images using `/api/v1/media` endpoint.

### Fixed

- Fixed post and profile page redirections.
- Fixed federation with GNU Social.
