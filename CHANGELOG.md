# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

## [4.14.0] - 2025-11-28

### Added

- Added `timelines_access` property to `/api/v2/instance` response ([#191](https://codeberg.org/silverpill/mitra/pulls/191)).
- Added API endpoints for session management.
- Allow adding subscribers when Monero integration is not enabled.

### Changed

- Write client name to log every time an access token is generated.
- Always verify client ID if provided in token request.
- Associate access tokens with OAuth applications.
- Changed default access token lifetime to 30 days.
- Verify public keys, identity proofs and payment options when reading profiles from database.
- Write warning to log if reaction is followers-only.

### Fixed

- Fixed remote invoices not expiring if Monero integration is not enabled.

## [4.13.1] - 2025-11-19

### Changed

- Ignore `quote` property in Bookwyrm quotations.

### Fixed

- Fixed panic on trying to save post to IPFS.

## [4.13.0] - 2025-11-13

### Added

- Added support for listening on IPv6 address.
- Sync activities in public conversations if `federation.fep_171b_public_enabled` is set to `true`.
- Sync reactions in conversations.

### Changed

- Treat public keys and verification methods as different core types.
- Changed priority of `Actor` to highest in duck typing algorithm.
- Prevent type confusion attacks when verifying FEP-ae97 activities.
- Allow `properties` with value `null` in JRD links.
- Return only supported scopes after creating application with `/api/v1/apps`.
- Write OAuth app scopes to log.
- Set `Poll.expires_at` to `null` if poll is without end date.
- Allow embedded `featured` collections.
- Do not allow replying with "Subscribers only" visibility to public posts.
- Force "Conversation" visibility if conversation scope is widened.
- Optimized thread retrieval query.
- Verify integrity proofs on FEP-1b12 wrapped activities.
- Use value of `summary` property as attachment description if `name` is not present.

### Fixed

- Fixed database error occurring when application `scopes` parameter is too long.
- Fixed M4A audio uploads not working in Bloat-FE ([#184](https://codeberg.org/silverpill/mitra/pulls/184)).

## [4.12.0] - 2025-10-26

### Added

- Set `discoverable` flag to `true` on Mastodon API `Account` entities.
- Replace `description` meta tag on post and profile pages.
- Added `/api/v1/instance/domain_blocks` API endpoint.
- Added `filter_blocklist_public` dynamic configuration parameter.
- Added `database_connection_pool_size` configuration parameter.
- Allow AAC, M4A and WAV audio uploads ([#180](https://codeberg.org/silverpill/mitra/pulls/180)).
- Added `expires_in` parameter to `/oauth/token` response.
- Added `scopes` and `redirect_uris` fields to `/api/v1/apps` response.

### Changed

- Changed data integrity context to `https://w3id.org/security/data-integrity/v2`.
- Use `eddsa-jcs-2022` cryptosuite when generating minisign identity proofs.
- Don't reject remote polls without end date.
- Improved documentation and error messages for `update-config` command.
- Make `get-config` display default value if configuration parameter is not set.
- Set `pleroma.parent_visible` flag to `false` on `Status` if parent post can't be displayed due to its visibility setting.
- Reduced database lock time during processing of remote posts.
- Reduced database lock time during followers/following import.
- Reduced database lock time during processing of remote actor profiles.
- Reduced database lock time during pruning of remote posts.
- Process uploads with `application/ogg` media type as `audio/ogg`.
- Strip query parameters when canonicalizing 'ap' URIs.
- Verify ownership of embedded objects submitted by FEP-ae97 clients.
- Allow empty `status` field in `/api/v1/statuses` form data.
- Add `Cache-Control: no-store` header to `/oauth/token` response.

### Fixed

- Use correct verification method ID when creating identity proofs.
- Fixed RFC-9421 signature verification error when `@request-target` is used.
- Prefer JSON over form data when deserializing ambiguous payloads.

### Security

- Verify client ID when issuing access token using authorization code flow.

## [4.11.0] - 2025-10-09

### Added

- Write warning to log if `instance_url` configuration parameter has incorrect URL scheme.
- Added configuration parameter `http_cors_allow_all`.
- Added FEP-ae97 API endpoint for deleting media.
- Added FEP-ae97 gateway metadata endpoint.
- Set `discoverable` flag to `true` on actors.
- Specify minimum payment amount in `Proposal` objects.
- Update status of a remote invoice when remote server receives payment.

### Changed

- Changed minimum required PostgreSQL version to 15.
- Don't include invoice and subscription counts in `instance-report` output if Monero intergration is not enabled.
- Renamed `instance_uri` configuration parameter to `instance_url` (`instance_uri` is treated as alias).
- Log CORS errors.
- Return `413 Payload Too Large` if media uploaded by portable actor is too large.
- Return `403 Forbidden` if media uploaded by an unregistered portable actor.
- Return `404 Not Found` if the owner of portable outbox is not registered.
- Return `401 Unauthorized` if the signature is not valid when pulling from portable inbox or outbox.
- Increased invoice timeout to 4 hours.
- Reduced database lock time during execution of payment monitor task.
- Write current number of transaction confirmations to log.
- Added `attributedTo` property to `Agreement` object.

### Fixed

- Copy port number from URL to `Host` header when creating HTTP signature.
- Fixed request URI reconstruction in case when `Host` header value includes port number.
- Fixed log-in when `password` flow is used and content is a multipart form.

### Security

- Prefer to read content digest from a signed header.

## [4.10.1] - 2025-09-21

### Added

- Added `tx_required_confirmations` parameter to Monero integration configuration.

### Changed

- Wait for 20 confirmations when processing incoming payments.

### Fixed

- Always add `payment_amount_min` parameter to chain metadata in `/api/v1/instance` response.

## [4.10.0] - 2025-09-16

### Added

- Download media attachment if `--attachment` argument of `create-post` command is a URL.
- Added support for media attachments to `import-posts` command.
- Added `--id` argument to `create-post` command.
- Add Atom feed link to `<head>` of the profile page.
- Added FEP-ae97 API endpoints for uploading media.

### Changed

- Changed MSRV to 1.75.0.
- Remove invisible characters from custom feed name before saving it to database.
- Validate custom feed name when updating custom feed.
- Write actor ID to log when processing a forwarded activity.
- Stop relying on `mediaType` property when processing remote media.
- Return standard "user not found" error from `create-post` if author is not local.
- Improved data validation in `import-posts` command.
- Generate deterministic post IDs when `create-post` and `import-posts` commands are used.
- Sort custom feeds by name.
- Improved descriptions of request errors produced by fetcher and deliverer.
- Reduced database lock time during processing of incoming activities.
- Reduced database lock time during verification of HTTP signatures.
- Reduced database lock time during reply tree retrieval.

### Fixed

- Fixed mention not being parsed if followed by a dot.
- Fixed logging of REST API authentication errors.
- Fixed potential deadlock in incoming queue worker.

## [4.9.0] - 2025-08-31

### Added

- Added `import-posts` command for importing archived posts.
- Fetch activities from origin if signer doesn't match actor.
- Added `/api/v1/admin/posts/{status_id}` API endpoint.
- Added `/api/v1/admin/accounts/{account_id}` API endpoint.

### Changed

- Write message to log when activity is added to portable actor's inbox.
- Reduced database lock time during collection import.
- Display detailed error message when portable object has invalid integrity proof.
- Generate FEP-171b activity when local post is deleted with `delete-post` command.

### Security

- Escape post title before inserting into page `<head>`.

## [4.8.0] - 2025-08-18

### Added

- Allow local usernames containing uppercase letters and hyphens ([#157](https://codeberg.org/silverpill/mitra/pulls/157)).
- Protected media proxy by requiring server's signature.
- Forward activities from a portable inbox to other gateways.

### Changed

- Write message to log if media proxy is enabled.
- Don't allow `.+-` in names of local custom emojis.
- Don't allow local custom emojis with `image/apng` media type.
- Allow remote custom emojis with `image/avif` and `image/jpeg` media types.
- Don't insert URL into post if type is `ChatMessage`.
- Always add group to reply audience when replying to post in group.
- Don't reject remote posts with a quote and no content.
- Enabled media proxy endpoint by default.
- Return at most 2 results when searching for a user as a guest.
- Respond with status code 404 if media can't be proxied.

## [4.7.0] - 2025-08-01

### Added

- Added support for `Create` activities without embedded object.
- Added `steal-emoji` alias for `import-emoji` command.
- Added `proxy-media` filter action.
- Added `media_proxy_enabled` configuration option.

### Changed

- Verify activity sender instead of actor when filtering spam.
- Ignore forwarded `Announce` activities.
- Optimized `Update` activity handler.
- Changed MSRV to 1.74.1.
- Added support for `@query` RFC-9421 component.
- Exit with non-zero code if validation error occurs in `add-emoji` or `import-emoji` command.
- Always validate actor profile data when it is retrieved from the database.

### Removed

- Removed `attributedTo` property from `Emoji` objects.

### Fixed

- Use original emoji `id` when generating `Like` / `EmojiReact` activity with remote emoji.
- Use correct file size limit when adding custom emoji by URL.
- Don't copy media attributes when stealing remote emoji.

### Security

- Require HTTP signatures to cover request method and target URI.

## [4.6.0] - 2025-07-16

### Added

- Enabled autolinking for `nex://` URIs.
- Display FEP-044f quotes.
- Signal support for RFC9421+Ed25519.
- Added `quote` property to Mastodon API `Status` entity.
- Added command that runs standalone queue workers.

### Changed

- Reduced database connection lock time in incoming activity queue executor and fetcher queue executor.
- Write detailed error message to log when payment forwarding fails.
- Enabled rendering of underlined text in instance descriptions.
- Disabled rendering of strikethrough text in bio.
- Allow anonymous collection pages.
- Don't escape HTML special characters in display name.
- Write message to log when Ed25519 key is used for HTTP signature.
- Remove zero-width characters from display names.
- Add `hidden` flag to `Status` entities in `links` array if the author is muted.
- Changed CSP to allow external connections.
- Sort profiles in profile directory by the date of the latest post.

### Fixed

- Fixed autolinking of URLs with IPv6 hostnames.
- Fixed post import failure when `quoteUrl` is not a canonical ID.
- Fixed parsing of consecutive `[[..]]` links.

## [4.5.1] - 2025-06-23

### Added

- Add `payment_amount_min` parameter to `chain_metadata` entity in `/api/v1/instance` response.
- Add information about pending invoices to instance report.
- Added `repair-invoice` command.

### Changed

- Write message to log before forwarding payment.
- Require subscription price to be greater than minimum payment amount.
- Write message to log when invoice status is changed to "timeout".
- Increased timeout of payment forwarding operation to 120 seconds

### Fixed

- Use compatible object IDs for web client redirections.
- Fixed description of `import-emoji` command.

## [4.5.0] - 2025-06-18

### Added

- Implemented `/api/v2/notifications/policy` API endpoint.
- Advertise RFC-9421 support using FEP-844e mechanism.
- Implemented `/api/v1/statuses/{status_id}/favourited_by` API endpoint.
- Implemented `/api/v1/pleroma/statuses/{status_id}/reactions` API endpoint.
- Support delivering portable activities to followers.
- Added `check-uris` command.

### Changed

- Raise database error in inbox/outbox views if stored actor ID is not canonical.
- Improved descriptions of authentication errors.
- Use same-actor policy when verifying inbox/outbox read permission.
- Improved documentation of `create-post` command.
- Disabled autolinking of email / webfinger addresses.
- Write detailed error message to log when verification of embedded portable object fails.
- Write message to log when incoming request contains RFC-9421 signature.
- Include file size limit in error message when uploaded file is too big.
- Replaced DID context with CID context in actor objects.
- Increased maximum number of poll options to 15.
- Support calling `/api/v1/accounts/lookup` with full local address.
- Return empty array to unauthenticated users when they call `/api/v1/statuses/{status_id}/reblogged_by`.
- Allow authenticated users to access reposts using `/api/v1/statuses/{status_id}/reblogged_by`.
- Make `/oauth/revoke` API endpoint accept form data.

### Removed

- Don't accept requests signed with DIDs.

### Fixed

- Don't parse `#1` as a hashtag.
- Added missing attributes to Mastodon API `Relationship` entity.
- Use compatible `object` ID when generating `Like` activity for portable object.

## [4.4.0] - 2025-06-05

### Added

- Allow requests to portable inboxes/outboxes signed with keys identified by 'ap' URLs.
- Added `webfinger` command.
- Sync `Update` and `Delete` activities in FEP-171b conversations.
- Support SHA-512 content digests.
- Support verification of signed requests with `Content-Digest` header.
- Verify RFC-9421 signatures.
- Implemented `/api/v1/statuses/{status_id}/reblogged_by` API endpoint.

### Changed

- Improved handling of database errors during OAuth token verification.
- Write details of database errors and internal errors to log when processing requests.
- Return plain text errors for routes other than Mastodon API.
- Changed MSRV to 1.72.1.
- Don't reject actor if associated public key has different origin.
- Set `Accept` header value to `application/jrd+json` when performing WebFinger query.
- Log error if full request URI can't be obtained during HTTP signature verification.
- Store visibility of post reactions in database.

### Removed

- Removed `account_ids` property in `Status.pleroma.emoji_reactions` items.
- Removed `federation.fep_e232_enabled` configuration parameter.

### Fixed

- Fixed WebFinger lookup not working on Yggdrasil addresses.
- Display followers-only reposts of viewer's posts in profile timeline.
- Don't add `Public` to `Like` activity audience.

## [4.3.1] - 2025-05-18

### Fixed

- Fixed `Content type error` when making a repost.

## [4.3.0] - 2025-05-18

### Added

- Added `get-config` command.
- Allow blockquotes in posts.
- Remove follow relationships when receiving `Block` activity.
- Support `Add(Delete)` activities.
- Support followers-only reposts.
- Support WebFinger addresses with IPv6 hostnames.

### Changed

- Changed activity limit of outbox fetcher to 20.
- Accept marker options formatted as JSON objects at `/api/v1/markers`.
- Deliver activities to one onion server at a time.
- Removed unnecessary database query in portable outbox handler.
- Don't include DMs in post count.
- Ignore actor's public keys that have different owner.
- Create reposts for `Article` objects addressed to FEP-1b12 groups.
- Reject `Update(Object)` activity if `inReplyTo` has changed.
- Write warning to log if scope widening happens in a remote reply.
- Reject objects with invalid audience.

### Fixed

- Fetch target actor when processing portable `Follow` activity.
- Add incoming activities to portable actor's inbox even if audience is not specified.
- Fixed error during processing of `Undo(Follow)` activity targeting portable actor.
- Don't clear extra fields when performing partial profile update with `multipart/form-data`.

## [4.2.1] - 2025-05-08

### Changed

- Don't reject portable actor objects without public keys.

### Fixed

- Prevent infinite loop when forwarding portable activities.

## [4.2.0] - 2025-05-06

### Added

- Added `delete-account` alias for `delete-user` command.
- Support loading featured posts using `/api/v1/accounts/{account_id}/load_activities` API endpoint.
- Added `limits.posts.attachment_limit` configuration parameter.
- Added `limits.posts.attachment_local_limit` configuration parameter.

### Changed

- Always validate activity audience.
- Ensure filtering is performed whenever object is fetched.
- Save activities to database only after processing them.
- Forward portable activities only after processing them.
- Don't ignore repeated activities.
- Track follow requests made by portable accounts.
- Support partial updates with `/api/v1/accounts/update_credentials`.
- Append attachment URLs to post when attachment limit is exceeded.
- Changed default attachment limit to 16.

### Deprecated

- Deprecated `delete-profile` alias of `delete-user` command.

### Fixed

- Fixed display of ">" in code blocks ([#134](https://codeberg.org/silverpill/mitra/pulls/134)).
- Fixed backslashes appearing near exclamation marks.
- Fixed broken portable actor registration endpoint.

### Security

- Don't save fetched portable objects with local compatible IDs.
- Don't serve portable activities if they are not validated.
- Prevent FEP-ae97 clients from creating keys that are not known to server.

## [4.1.1] - 2025-04-27

### Added

- Added `federation.fep_ef61_gateway_enabled` configuration option.

### Changed

- Disabled FEP-ef61 gateway by default.
- Treat verification methods as separate core type.

### Security

- Don't serve portable objects that are not owned by actors with local accounts.
- Ignore portable activities from remote servers with local compatible IDs.

## [4.1.0] - 2025-04-26

### Added

- Make available space indication work on FreeBSD.
- Resolve fragments when fetching objects.

### Changed

- Accept poll options formatted as JSON object at `/api/v1/statuses`.
- Require GET requests to portable inboxes and outboxes to be signed with DIDs.
- Generate HTTP signature keys when registering portable actor.
- Log internal profile ID of portable actor if it existed before registration.
- Log emoji alternate name when it is present.

### Fixed

- Don't reject public posts from portable users as spam.
- Fixed panic in remote vote handler.

## [4.0.0] - 2025-04-17

### Changed

- Set minimum migration version to 72.
- Disabled custom migration that generates Ed25519 keys for accounts.
- Treat scheduled outgoing activities where sender is not specified as invalid.
- Removed unhashed OAuth tokens from database.
- Don't change default config file path when `production` feature is enabled.
- Changed default data retention parameters.
- Changed default fetcher and deliverer timeouts.
- Renamed `reject` filter action to `reject-incoming`.
- Renamed `reject-data` filter action to `reject`.
- Changed default value of `object-type` parameter of `import-object` command to `any`.
- Renamed `mitractl` binary to `mitra`.

### Deprecated

- Deprecated `blocked_instances` and `allowed_instances` configuration parameters.

### Removed

- Don't read instance key from `instance_rsa_key` file.
- Removed `import-actor`, `import-activity` and `prune-remote-emojis` commands.

### Fixed

- Don't panic on non-critical errors when executing commands.

## [3.23.0] - 2025-04-15

### Added

- Allow changing default configuration file path at compile time.
- Create reposts when processing `Announce(Create(Note))` for top-level group posts.
- Support per-post language selection.

### Changed

- Log HTTP server errors where error source is unknown.
- Don't make requests to blocked instances when `import-object` command is used.
- Use `https://w3id.org/valueflows/ont/vf#Proposal` relation type for subscription payment links.
- Show recently updated profiles first in search results.
- Added `reject-incoming` alias for `reject` filter action.
- Compare identifiers instead of actors when verifying wrapped FEP-1b12 activities.

### Deprecated

- Deprecated `prune-remote-emojis` command.

### Removed

- Dropped support for `mitra-jcs-rsa-2022` cryptosuite.

### Fixed

- Fixed internal server error when acessing rate-limited endpoints via unix socket.
- Fixed incorrect rate limiter IP address check.
- Fixed incorrect de-duplication of activity recipients.
- Fixed broken uploads in Bloat-FE.

## [3.22.0] - 2025-04-02

### Added

- Added `/api/v2/filters` API endpoint returning empty array.
- Support ~~strikethrough~~ text in posts.
- Added `/api/v1/notifications/clear` API endpoint.
- Allow adding "automated" flag to account.

### Changed

- Changed the value of `Role.permissions` to bitmask as specified in Mastodon documentation.
- Allow POSTing form-encoded data to `/api/v1/markers`.
- Added documentation for `create-post` command.
- Mark portable actor as reachable if at least one inbox is reachable.
- Don't retry delivery if status is `410 Gone`.
- Prioritize primary recipients when delivering posts.
- Add `privacy` and `sensitive` properties to `Account.source`.
- Don't make requests to blocked instances when verifying FEP-1b12 and FEP-171b activities.
- Compare canonical IDs when verifying FEP-c390 proof.
- Speed up full text search.

### Fixed

- Changed encoding of OpenMetrics data to UTF-8.
- Fixed metadata in OpenMetrics exposition.
- Added EOF to OpenMetrics exposition.
- Fixed incorrect sorting of backdated posts.
- Fixed `profile not found` when processing `Announce(Undo)`.

## [3.21.0] - 2025-03-24

### Added

- Accept remote identity proofs created using `eddsa-jcs-2022` cryptosuite.
- Added FEP-3b86 `Object` intent to WebFinger links.
- Added `federation.fep_c0e0_emoji_react_enabled` configuration option.
- Added OpenMetrics endpoint for monitoring queue sizes.

### Changed

- Check origin of verification method when verifying activity signature.
- Write message to log when server stops.
- Copy `role.permissions` to `role.permissions_names` in `Account` Mastodon API entity.
- Optimized related posts query.
- Optimized profile timeline, featured collection and conversation context queries.
- Replace actor key ownership validation warnings with errors.

### Fixed

- Try to fix application hang on shutdown.

## [3.20.0] - 2025-03-15

### Added

- Added `server` command to mitractl.
- Support `Announce(Undo)` activities.
- Added `reject-keywords` filter action.

### Changed

- Ensure media directory exists when `mitractl` runs.
- Write instance URL to log when `mitractl` runs.
- Mark `import-actor` and `import-activity` commands as deprecated.
- Allow replying with "conversation" visibility to author's own followers-only post.
- Disallow uppercase letters in HTTP URL host component.
- Compare origins instead of hostnames when verifying fetched non-portable object.
- Make validation of `instance_uri` configuration parameter more strict.
- Store actor's shared inbox.
- Hide deprecated filter actions from `add-filter-rule` help message.
- Describe each filter action in `add-filter-rule` help message.
- Improved formatting of `update-config` help message.

### Fixed

- Fixed bug in processing of FEP-171b activities.
- Generate `Add` activity when conversation owner posts a reply.
- Avoid errors when parsing URLs stored in database.

## [3.19.0] - 2025-03-09

### Added

- Added collection support to `import-object` command.
- Added `mark-sensitive` filter action.
- Automatically remove unused tags from database.
- Add OpenGraph metadata to profile pages.

### Changed

- Don't load replies collection when context is requested.
- Refresh cached posts when loading thread or replies.
- Replace "scheduler" with "background worker" in log messages.
- Changed default value of `limits.media.emoji_size_limit` to 1 MB.
- Reverted to allowing single character emoji shortcodes.
- Don't pollute PostgreSQL log with `post_reaction` table constraint violation errors.
- Don't import post when `reject-data` rule for target domain exists.
- Drop delivery if `reject-data` rule for target domain exists.

### Fixed

- Add newline to `get-payment-address` command output.

## [3.18.0] - 2025-02-26

### Added

- Support conversation backfilling via `contextHistory` property.
- Added `http_socket_perms` configuration parameter.
- Added `profile_image_local_size_limit` configuration parameter.
- Implemented `context` collection.
- Added `prune-reposts` command.
- Added `create-post` command.

### Changed

- Changed local profile image size limit to 2 MB.
- Ignore local activities delivered via `Add` and `Announce`.
- Require emoji shortcodes to have at least two characters.
- Use compatible object IDs in `replies` collection.
- Improved performance of `/objects/{post_id}` view.
- Changed subpath of FEP-171b collection to `/history`.
- Write message to log when encountering unsupported tag on actor object.
- Update remote emoji object ID if name and hostname match.
- Check post content length after HTML sanitization.

### Removed

- Removed `--use-container` option of `load-replies` command.

### Fixed

- Fixed incorrect processing of poll options with HTML special characters.
- Use correct datetime format in Mastodon API responses.

## [3.17.0] - 2025-02-13

### Added

- Parse custom emojis in poll options.
- Support EdDSA HTTP signatures.
- Add option to import activities with `import-object` command.
- Add option to import actors with `import-object` command.
- Support remembering home timeline position via `/api/v1/markers`.
- Added `reject-data` filter action.

### Changed

- Use `Like` activity to represent emoji reactions.
- Don't prune posts that contain polls with local votes.
- Reverse order of rules in `list-filter-rules` command output.
- Don't allow hashtags inside links.
- Changed MSRV to 1.70.0.
- Make multipart memory limit match total payload limit.

### Deprecated

- Deprecated `import-activity` command.
- Deprecated `import-actor` command.

### Fixed

- Removed spurious warnings about vote content being not null.
- Don't mark post as edited unless content and attachments are changed.
- Fixed processing of replies with narrowed down audience as DMs.

## [3.16.0] - 2025-02-03

### Added

- Allow to edit posts with polls.

### Changed

- Write warning to log when `content` of a vote is not null.
- Changed MSRV to 1.68.2.
- Write warning to log if actor and actor key have different origins.
- Reject portable actors without WebFinger address.
- Truncate long display names instead of rejecting actor.
- Include URL in unsafe URL error message.
- Reject poll options containing HTML.
- Sanitize HTML in poll options when creating poll.
- Enable incoming queue worker by default.

### Fixed

- Change poll max expiration time to avoid integer overflow in Mastodon clients.
- Preserve order of mentions in local posts.
- Fixed violation of data integrity when portable account is updated.
- Fixed redirection error when `Location` is a relative URL.
- Verify that posts quoted by remote actors are public.
- Fixed panic when voting on poll in reply.

## [3.15.0] - 2025-01-24

### Added

- Implemented poll publishing.
- Added poll limits to `/api/v*/instance` response.

### Changed

- Changed MSRV to 1.67.1.
- Count physical cores when calculating database connection pool size.
- Disallow duplicated poll options.
- Preserve order of mentions.
- Convert FEP-fb2a links to profile fields regardless of relation type.

## [3.14.0] - 2025-01-14

### Added

- Allow to run separate worker for processing of incoming activity queue.
- Support binding HTTP server to unix socket.
- Process incoming `Add(Update)` activities.

### Changed

- Log object ID if poll data is not valid.
- Log database errors coming from inbox handler with level ERROR.
- Log database connection pool size.
- Release database connection before delivering activity.
- Set value of `Status.url` to actual post URL if it is known.
- Don't write warning to log if alias is equal to actor ID.
- Allow to call `/api/v1/markers` with multiple `timeline[]` parameters.

### Fixed

- Allow to use `:+1:` emoji shortcode.
- Added workaround for PeerTube `keyId` bug.

## [3.13.1] - 2025-01-03

### Fixed

- Fix monero transfer error when monero-wallet-rpc 0.18.3.4 is used.

## [3.13.0] - 2024-12-28

### Added

- Support subscribers-only conversations.
- Sync replies in followers-only and subscribers-only conversations between participants.
- Add `poll` attribute to Mastodon API `Status` entity.
- Implemented `/api/v1/polls/{poll_id}/votes` API endpoint.
- Added `--as-user` parameter to `import-object` command.

### Changed

- Return proper error when attempting to fetch local actor.
- Improved validation of remote polls.
- Ignore local activities received via inbox.
- Respect "Hide reposts" and "Hide replies" settings in custom feeds.
- Canonicalize IDs in `to` and `cc` fields before processing them.
- Don't write additional message to log when processing `Create(Question)` activity.

### Fixed

- Don't panic if attempting to set "Conversation" visibility on top-level post.

## [3.12.0] - 2024-12-19

### Added

- Added `pleroma.metadata.fields_limits` to `/api/v1/instance` response.
- Added `reject-profile-images` filter action.
- Added `reject-custom-emojis` filter action.
- Make profile image size limit configurable.
- Update profile bio, lock and fields when processing `update_credentials` request with `multipart/form-data` body.
- Added `revoke-oauth-tokens` command.
- Support followers-only conversations.

### Changed

- Sniff upload media type if not provided by client.
- Don't write unnecessary delivery reports to log when in private mode.
- Panic if trying to migrate from version 1.36.0 or older.
- Rename `reject-media` filter action to `reject-media-attachments`.
- Ensure wildcard filter rules have lowest priority.

### Deprecated

- Deprecate `reject-media` alias of `reject-media-attachments` filter action.

### Removed

- Removed exotic URI schemes from HTML sanitizer allowlist (geo, im, mms and others).

### Fixed

- Remove `Content-Encoding` header only when path starts with `/media`.
- Don't autolink URIs with unsupported URI schemes.
- Don't reject fetched AP object if profile value in content type is not quoted.

## [3.11.0] - 2024-12-05

### Added

- Added `import-object` command.
- Add `digestMultibase` property to media attachments.

### Changed

- Changed MSRV to 1.66.1.
- Improved performance of `delete-orphaned-files` command.
- Renamed `fetch-activity` command to `import-activity`.
- Renamed `fetch-actor` command to `import-actor`.
- Store hashed OAuth access tokens.
- Changed OAuth authorization code lifetime to 5 minutes.
- Log `redirect_uri` and `client_id` parameters when OAuth access token is requested.
- Save media digests to database.
- Overwrite cached activity if attributed object has same ID.
- Keep activities in portable inboxes and outboxes for 90 days.
- Use `Image` type to represent attached images.
- Support attachments where `url` is a `Link` object.

### Fixed

- Fixed incorrect log message when reply can not be imported.
- Fixed owner check in `Update(Note)` handler.
- Fixed broken custom emojis in post preview at onion mirror.
- Fixed broken media attachments in post editor at onion mirror.

## [3.10.0] - 2024-11-20

### Added

- Added `--dry-run` parameter to `delete-orphaned-files` command.
- Support fetching conversation containers with `fetch-replies` command.
- Support `__underlined__` text in post content.
- Added `content_type` property to `StatusSource` Mastodon API entity.

### Changed

- Improved performance of `delete-orphaned-files` command.
- Require verification methods to be valid RFC-3986 URIs.
- Require `keyId` HTTP signature parameter to be valid RFC-3986 URI.
- Increased number of fetched collection pages to 3.
- Verify origins of collection pages.
- Rename `fetch-replies` command to `load-replies`.
- Don't drop remote posts containing too many hashtags.
- Don't write repeated messages to log when tag count exceeds limit.
- Validate OAuth redirect URIs according to RFC-3986.
- Return status 400 if WebFinger resource parameter is not valid.
- Log Mastodon API server errors with level `ERROR`.
- Log Mastodon API client errors.
- Log OAuth client errors.
- Return deleted `Status` entity when processing `DELETE /api/v1/statuses/{status_id}`.
- Don't repeat delivery error messages.

### Fixed

- Fixed validation of FEP-1b12 activities where `object.actor` is embedded.
- Prevent removal of attachments during post editing in some Mastodon API clients.

### Security

- Rate-limit `/oauth/token` API endpoint.

## [3.9.0] - 2024-11-07

### Added

- Allow level 1 markdown headings in posts.
- Add `published` and `updated` properties to actor documents.
- Added `--skip-verification` parameter to `fetch-object` command.
- Support "conversation" visibility for replies to followers-only posts.
- Add `og:site_name` and `og:type` directives to post page metadata.
- Add post title to `og:title` value.
- Added `federation.deliverer_pool_size` configuration option.

### Changed

- Improved error reporting during activity parsing.
- Support portable `Update(Note)` activities.
- Update cached ActivityPub object when processing `Update(Object)` activity.
- Reject `Create` activity if actor doesn't match object owner.
- Allow followers-only self-replies to followers-only posts.
- Don't add author to mentions and audience when publishing a self-reply.
- Improved logging of media deletions.
- Panic if directory specified in `web_client_dir` does not exist.
- Run delivery worker separately from other background tasks by default.

### Fixed

- Limit results to posts made by current user when checking idempotency key.
- Don't remove mention of a parent post author when editing a reply.

## [3.8.0] - 2024-10-25

### Added

- Automatically delete media attachments that were not attached to posts.
- Added `new_accounts_read_only` field to `/api/v1/instance` response.
- Added `fetch-activity` command.
- Support `Add(EmojiReact)` conversation container activities (incoming).
- Support `Add(Create)` conversation container activities (incoming).
- Include portable accounts in `list-accounts` command output.
- Allow running delivery worker separately from other background tasks.

### Changed

- Enable markdown autolinks for `gemini://` URL scheme.
- Don't pollute PostgreSQL log with `activitypub_object_pkey` constraint violation errors.
- Improved error reporting during actor document parsing.
- Optimized post queries.
- Improved documentation of `list-filter-rules` command.
- Retry incoming activities only on fetcher errors.
- Don't fetch object of FEP-1b12 announced `Create` and `Update` activities.
- Update actor reachability statuses with a single query.
- Don't publish post if `Idempotency-Key` header value is reused.
- Ignore forwarded unsigned `EmojiReact` activities.

### Fixed

- Try to fix `background job not found` error that occurs in multi-process setup.

## [3.7.1] - 2024-10-17

### Changed

- Log portable actor registration errors.
- Improved container activity authentication error reporting.

### Fixed

- Fix registration of portable actors.

## [3.7.0] - 2024-10-16

### Added

- Added `create-account` alias to `create-user` command.
- Added `list-accounts` alias to `list-users` command.
- Added `delete-user` alias to `delete-profile` command.
- Added information about supported post formats to `/api/v1/instance` response.
- Added `pleroma` object to `/api/v2/instance` response.
- Support storing blocklist and allowlist configuration in database.
- Support remote media filtering.

### Changed

- Display better error message when request signer is not found in local cache.
- Fetch conversation container item if integrity proof is not present.
- De-duplicate media attachments by URL.
- Use primary gateway address for filtering messages from portable actors.
- Ignore images in `icon` field if object type is not `Video`.
- Store URLs of remote media.
- Delete orphaned image files after updating emoji.
- Accept `Emoji` objects without `id`.

### Fixed

- Canonicalize IDs when parsing `inReplyTo` and object links.
- Fixed rendering of "Post not found" page.
- Fixed incorrect error message when object signer doesn't match owner.
- Fixed miscategorization of portable replies as spam.

## [3.6.0] - 2024-10-03

### Added

- Insert OpenGraph tags into `index.html` when serving post page.
- Added API endpoint for editing custom feed name.
- Support `Add(Like)` and `Add(Dislike)` conversation container activities.
- Support `Announce(Update)` activities.
- Added `/api/v1/accounts/{account_id}/lists` API endpoint.

### Changed

- Don't duck-type collections as "objects".
- Display reposts made by current user in home timeline (reverting change made in v3.4.0).
- Allow `proof.verificationMethod` to be DID URL.
- Write message to log if FEP-1b12 activity is not supported.
- Don't fetch embedded FEP-1b12 activity if it has same origin as its parent.
- Changed MSRV to 1.65.0.
- Accept 'ap' URLs with percent-encoded authority.
- Support portable `Like` activities.

### Fixed

- Fixed incorrect processing of `created` value during FEP-8b32 proof verification.

## [3.5.0] - 2024-09-17

### Added

- Implemented Mastodon List API.
- Display PeerTube video thumbnails as attachments.
- Include posts to which current user reacted in full text search results.
- Include posts where current user is mentioned in full text search results.

### Changed

- Moved library modules from `mitra_utils` to `apx_core` package.
- Renamed `mitra_federation` package to `apx_sdk`.
- Allow mentions and hashtags with multiple stop characters after them.
- Enable FEP-e232 by default.
- Don't remove paragraphs when editing bio.
- Sort bookmarks by creation date.
- Federate 👎 reaction as `Dislike` activity.
- Optimize generation of `replies` collection.
- Increased delay before first delivery retry.

### Removed

- Removed `node_url` parameter from Monero configuration.

### Fixed

- Don't rate-limit authenticated calls to `/api/v1/accounts/search`.
- Fixed incorrect reporting of object ID errors during fetching.

## [3.4.0] - 2024-09-04

### Added

- Generate notification when subscriber is leaving.
- Added `federation.ssrf_protection_enabled` configuration parameter.
- Include bookmarked posts in full text search results.

### Changed

- Allow reactions with remote emojis.
- Store bookmark creation date.
- Display better error message when config file is not found.
- Enable identicon caching.
- Don't serve `index.html` if `assets/custom.css` doesn't exist.
- Improved formatting of "available space" message.
- Display better error message when imported emoji is too big.
- Delete `instance_rsa_key` file after copying key into database.
- Don't display reposts made by current user in home timeline.
- Ignore non-embedded activities when loading outbox.
- Increase delivery pool size from 5 to 10.

### Fixed

- Fixed FK constraint violation error on profile update.

## [3.3.0] - 2024-08-28

### Added

- Added `list-local-files` command.
- Support adding custom emojis to profile description.
- Added API endpoints for bookmark management.

### Changed

- Add `'` and `;` to the list of stop characters for mentions and hashtags.
- Don't allow HTML tags in display name.
- Add missing `url` value to Mastodon API `Status` entities.
- Determine webfinger address when importing portable actors.
- Use compatible actor IDs when generating `Like` and `Undo(Like)` activities.
- Generate identicons for users without avatar image.
- Generate empty banner for users without banner image.
- Represent reply notification as `mention` with `reply` subtype.
- Don't log content of repeated activities.
- Ignore invalid `icon` and `image` values on actor.
- Changed order of commands in mitractl help text.

### Removed

- Removed `enable-fep-ef61` command.

### Fixed

- Don't drop `Announce(Dislike)` activities.
- Enforce ordering of items in `@context`.

## [3.2.0] - 2024-08-18

### Added

- Enabled pagination of search results.
- Added `/api/v1/mutes` Mastodon API endpoint.
- Add `pleroma.parent_visible` attribute to Mastodon API `Status` entity.
- Support `Announce(Dislike)` activities.

### Changed

- Log failed PostgreSQL version check with `ERROR` level.
- Provide better error message when available space can't be determined.
- Set `me` parameter of emoji reaction to `true` if it was made by current user.
- Don't include emoji reactions in favourites count.
- Change `import-emoji` command to accept shortcodes.
- Log all incoming activities when log level is set to "debug".
- Use `tag` array in `EmojiReact` activity.
- Improve logging of C2S authentication errors.
- Allow `mumble:` URIs in post content.
- Improved logging of processed activities.

### Deprecated

- Deprecated `federation.fep_1b12_full_enabled` configuration parameter.

### Removed

- Removed support for FEP-c390 + (old) FEP-ae97 activity authentication.
- Removed ethereum subscriptions from database.

### Fixed

- Fixed database error that occurred when trying to undo like.
- Don't generate OpenGraph response for non-public posts.
- Fixed remote invoices never timing out.

### Security

- Don't allow federation requests to private IP addresses.

## [3.1.0] - 2024-08-08

### Added

- Support viewing polls with multiple choices.
- Support fetching context collection with `fetch-replies` command.
- Process comments announced by FEP-1b12 implementations.
- Added `web_client_theme_dir` configuration parameter for replacing web client assets with custom ones.
- Allow overriding `http_port` configuration parameter using `HTTP_PORT` environment variable.

### Changed

- Display number of deleted files when running `delete-orphaned-files` command.
- Don't validate activities from blocked instances.
- Accept `Remove` activities with partially embedded target.
- Use duck typing for detecting FEP-1b12 `Announce` activities.
- Don't process `Announce` activities more than once.
- Rename `federation.announce_like_enabled` configuration parameter to `federation.fep_1b12_full_enabled`.
- Don't log `Announce(Like)` result if reaction was not created.
- Changed MSRV to 1.64.0.
- Improve logging of database errors during authentication.

### Fixed

- Fix processing of `Add(Note)` activities.
- Fixed re-fetching of nomadic actors.
- Use correct level when logging background fetcher errors.

## [3.0.0] - 2024-07-22

### Changed

- Don't panic if `blockchain` configuration parameter is present.
- Changed default log level in `mitractl` to `WARN`.
- Stop accepting `Add(Person)` activities without `endTime` property.
- Stop accepting proposals without `purpose` property.
- Don't add `provider` and `receiver` properties to `Intent` objects.
- Return error if PostgreSQL version check fails.
- Changed default value of `federation.fetcher_timeout` config parameter to `60`.
- Disallow Ethereum chains in `blockchains` array in configuration file.
- Write warning to log if `instance_rsa_key` file is present.

### Deprecated

- Deprecate `mitra-jcs-rsa-2022` cryptosuite.
- Deprecate FEP-c390 + old FEP-ae97 authentication.

### Removed

- Removed token gate.
- Removed Ethereum subscriptions.
- Disabled Etherem blockchain synchronization.
- Removed `update-current-block` command.
- Removed `generate-ethereum-address` command.
- Removed `contract_address`, `features.gate` and `features.miner` from instance info.
- Dropped support for Mitra 1.x outgoing queue data format.
- Removed support for `authentication` array in actor objects.
- Removed support for `clauses` property in `Agreement` object.
- Removed `daemon_url` alias of `node_url` parameter in Monero integration configuration.
- Removed `wallet_url` alias of `wallet_rpc_url` parameter in Monero integration configuration.
- Removed `generate-rsa-key` command.
- Removed `native-tls` and `rustls-tls` features.

### Fixed

- Fix incorrect payment link `rel`.

## [2.26.0] - 2024-07-20

### Added

- Write amount of disk space available for media to log at startup.
- Implemented remote interaction with posts.
- Added portable outbox view.
- Support changing mitractl log level using `--log-level` parameter.
- Insert poll results into post as text.
- Support adding custom emojis to display name.

### Changed

- Do not remove posts by muted users from threads.
- Removed `MitraJcsRsaSignature2022` from default `@context`.
- Added `Emoji` to default `@context`.
- Don't discard actor if it has more than 10 aliases.
- Return instance actor JRD if Webfinger is queried with instance base URL.
- Validate port number when parsing 'http' URLs.
- Replace "same authority" checks with "same origin".
- Discard portable actor if ID of a special collection has different origin than actor ID.
- Always use less verbose logging level for actix.

## [2.25.1] - 2024-07-11

### Fixed

- Prevent panic if actor ID changes during profile refresh.

## [2.25.0] - 2024-07-10

### Added

- Forward portable activities from outbox to actors listed in `to` and `cc`.
- Forward portable activities from outbox to other actor's clones.
- Implement S2S inbox endpoints for portable users.
- Added `pleroma.quote` property to Mastodon API `Status` entity.
- Support adding quote to post using `quote_id` parameter.
- Added `/api/v1/statuses/{status_id}/load_conversation` API endpoint.
- Support searching posts and profiles by `ap://` URL.
- Allow portable actors to send activities to regular inboxes.

### Changed

- Don't discard actor object if "icon" property value is a string.
- Signal support for Pleroma features via `/api/v1/instance` info.
- Enable rate limiting for `/api/v1/accounts` API endpoint.
- Don't refresh portable actors that have local accounts.
- Process activities submitted to outbox only once.
- Use compatible target actor ID when building `Follow` and `Undo(Follow)` activities.
- Added `pleroma.in_reply_to_account_acct` field to Mastodon API `Status` entity.

### Fixed

- Ignore delivery to local inbox if it doesn't exist.

## [2.24.0] - 2024-06-30

### Added

- Support importing remote portable actor profiles.
- Allow registration of portable actors with ap:// IDs.
- Support calling `/api/v1/accounts/{account_id}/statuses` with `exclude_reblogs` parameter.
- Serve portable attributed objects stored in database.

### Changed

- Accept `Update(Person)` C2S activities.
- Disable same-origin check if fetched object is portable.
- Log activities submitted to outbox.
- Don't log canonical object ID on FEP-ef61 object verification.
- Return `405 Method Not Allowed` if client can't POST to outbox.
- Don't raise error if actor's webfinger hostname is not known.
- Verify that keys provided during registration of portable user are present in actor document.
- Write app version to log before applying migrations.
- Use first line of content to create title for Atom feed entry.
- Don't fetch object of Create activity when it is portable and valid.

### Removed

- Removed FEP-c390 C2S outbox.

### Fixed

- Fix incorrect handling of outgoing delivery result.
- Use compatible ID when setting `inReplyTo` value.
- Fix incorrect error message during verification of `Create` and `Update` activities.

### Security

- Reject object if ID and owner have different authority.

## [2.23.0] - 2024-06-23

### Added

- Enable caching of remote portable actors.
- Implement registration endpoint for FEP-ef61 clients.
- Serve portable actor objects stored in database.
- Implemented C2S inbox for portable users.
- Put outgoing activities into inbox if portable recipient has local account.
- Implemented C2S outbox for portable users.
- Implemented Webfinger for portable users.

### Changed

- Save imported actor objects to database.
- Block deletion of user account if profile is not deleted.
- Removed profile-page entry from instance actor JRD.

### Removed

- Removed `/api/v1/accounts/signed_update` API endpoint.
- Removed FEP-ef61 representations of local actors.

### Fixed

- Don't reject actors with empty `PropertyValue` names.

### Security

- Block all requests to loopback addresses during fetch and delivery.

## [2.22.0] - 2024-06-15

### Added

- Add `attributedTo` property to `Emoji` objects.
- Support integrity proofs with injected context.
- Serve outgoing public activities.
- Added `federation.inbox_queue_batch_size` configuration parameter.
- Check PostgreSQL version at startup.
- Support search with `type` parameter.

### Changed

- Use rustls when building docker image.
- Don't re-fetch actor when reading outbox.
- Improve ownership check in `Update(Note)` handler.
- Write warning to log when object reference is invalid.
- Verify that profile linked to user account is local.
- Update Webfinger endpoint to return 404 Not Found when actor ID is unknown.
- Enable rustls by default.
- Validate IDs of incoming activities.
- Write message to log when processing forwarded activities.
- Silently ignore `Listen` activities from Pleroma.
- Drop activity if integrity proof is invalid.
- Save incoming and outgoing activities to database.
- Use new activity ID format.
- Changed worker delay from 5 seconds to 500 milliseconds.
- Run federation queue executors every 1 second.
- Improved validation of hostname part in profile search queries.
- Prevent remote invoices from being processed as local.

### Deprecated

- Deprecate OpenSSL support.

### Removed

- Removed `federation.fep_8b32_eddsa_enabled` configuration parameter.

### Fixed

- Fixed panic when searching for handle with invalid hostname.

## [2.21.0] - 2024-06-01

### Added

- Add sizes of data import and fetcher queues to instance report.
- Allow multiple emoji reactions on a single post.
- Implement `/api/v1/pleroma/statuses/{status_id}/reactions` API endpoints.
- Added rustls support.

### Changed

- Increased incoming activity queue batch size to 20.
- Change type of emoji reaction notification to `pleroma:emoji_reaction`.
- Set limit on multipart form size.
- Verify and drop portable activities sent to inbox.
- Don't allow replies to posts that user can't view.
- Don't count emoji reactions when determining "favourited" state of Status entity.
- Validate emoji shortcode in custom emoji reaction content.

### Removed

- Remove `sameAs` property from portable actor objects.
- Don't add `Link` header when serving portable objects.

### Fixed

- Prevent self-follow when moving followers to another account.
- Fixed incorrect response format when `Authorization` header is missing.
- Fixed incorrect format of Mastodon API validation errors.

### Security

- Don't allow reactions if user can't view post.

## [2.20.2] - 2024-05-29

### Added

- Added `federation.announce_like_enabled` configuration option.

### Changed

- Make database client errors more detailed in log.
- Optimize processing of `Announce(Like)` activities.

## [2.20.1] - 2024-05-29

### Fixed

- Prevent self-following when importing follow list.

## [2.20.0] - 2024-05-28

### Added

- Added `load-portable-object` command.
- Perform proof verification if fetched object is portable.
- Added `pleroma.emoji_reactions` list to `Status` entity.
- Added `/api/v1/settings/move_followers` API endpoint (replaces deprecated one).

### Changed

- Save gateway lists to database.
- Always add author of the parent post to mentions.
- Order emojis in local collection by name.
- Ensure compatible 'ap' URLs are not saved to database.
- Reject HTTP signature if key ID doesn't match one indicated by `publicKey` property.
- Add `emoji` and `emoji_url` fields to `Notification` entity for compatibility with Pleroma.
- Address `Move` activity to `Public` and followers collection instead of individual actors.
- Do not allow more than 10 identity proofs.
- Do not allow more than 10 aliases.
- Do not allow adding local aliases.
- Add declared aliases to recipient list of `Update(Person)` activity.

### Removed

- Removed `/api/v1/settings/move_followers` API endpoint.

### Fixed

- Fix URL search when URL contains a DID.

## [2.19.0] - 2024-05-22

### Added

- Support mentions containing internationalized domain names.
- Add `gateways` property to portable actor objects.
- Download image with `add-emoji` command if URL is provided.

### Changed

- Improved reporting of local ID parsing errors.
- Replace `fetch-object-as` and `fetch-portable-object` commands with `fetch-object` command.
- Require object IDs to be URIs.
- Normalize URLs when searching profiles/posts.
- Normalize hostnames when searching profiles.
- Do not allow unicode in 'acct' URIs and handles.
- Ensure hostname is properly encoded before saving profile or emoji to database.
- Don't ignore empty profile field names.
- Show local accounts first when searching for profiles.
- Don't retry FEP-1b12 activities if wrapped activity can't be fetched.

### Fixed

- Do not stop follow importer task on validation error.
- Fixed PieFed webfinger address resolution.
- Fixed URL component encoding.
- Don't discard actor if one profile field is not valid.

## [2.18.0] - 2024-05-07

### Added

- Added `/api/v1/settings/import_followers` API endpoint (replaces `/move_followers`).
- Support `Announce(Like)` activities.
- Added `/api/v2/instance` API endpoint.
- Support resolution of `ap` URLs in `fetch-portable-object` command.

### Changed

- Do not retry incoming activity if access to referenced object is denied.
- Allow `fetch-object-as` command to be called without username argument.
- Display better error message if fetched object doesn't have an ID.
- Set `Relationship.muting_notifications` flag to True if account is muted.
- Add `blocking` and `blocked_by` properties to Mastodon API Relationship object.
- Increase response size limit to 2MB.
- Added partial support for `multipart/form-data` payloads in `/api/v1/accounts/update_credentials`.

### Deprecated

- Deprecate `/api/v1/settings/move_followers` API.

### Fixed

- Fixed content encoding errors when serving media files.
- Fixed key ID to actor ID transformation for microblog.pub.
- Do not discard object if emoji ID is not valid.

### Security

- Re-fetch announced FEP-1b12 activities.

## [2.17.1] - 2024-04-29

### Changed

- Increased attachment description limit to 3000 bytes.

### Fixed

- Don't reject whole activity if attachment description is not valid.

## [2.17.0] - 2024-04-27

### Added

- Make local emoji size limit configurable.
- Added `/api/v1/accounts/{account_id}/remove_from_followers` API endpoint.
- Implemented remote interaction via Webfinger.
- Support searching for acct: URIs.

### Changed

- Make `note` property non-nullable in Mastodon API `Account` object.
- Renamed `apresolver` well-known endpoint to `apgateway`.
- Don't write warning to log if actor's public key changes.
- Increase importer limit from 50 items to 500.
- Use `ap://` URLs instead of `did:ap` URLs.
- Add canonical actor ID to `sameAs` array in portable actor objects.
- Do not follow redirects on activity delivery.
- Change default local emoji size limit to 256 kB.
- Validate all object IDs before saving them to database.
- Accept `off`, `on`, `0` and `1` as valid boolean values in Mastodon API queries.
- Allow `actor` property of `Like` and FEP-1b12 `Announce` activities to contain object.
- Verify that object is not an actor when importing it as a post.

### Fixed

- Fixed parsing of emoji shortcodes.
- Re-sign fetcher request after redirection.
- Don't replace emojis inside words.

## [2.16.0] - 2024-04-16

### Added

- Added `fetch-portable-object` command.
- Enabled full text search (limited to posts created by the current user).
- Added API endpoint for loading activities from remote actor's outbox.

### Changed

- Don't add total number of files to `delete-orphaned-files` command output.
- Added `id` field to Mastodon API `Application` object.
- Improve validation of object IDs.
- Validate actor ID and inbox URL before saving actor profile to database.
- Validate key IDs before saving actor profile to database.
- Write warning to log if key ID lookup fails.
- Use job queue to run follows and followers import procedures.
- Send migration notification to local followers.

### Removed

- Stop attaching legacy identity proofs to actors.

### Fixed

- Fixed incorrect un-following of a remote actor during migration.
- Fixed "user not found" error in `Move()` activity handler.

## [2.15.1] - 2024-04-09

### Removed

- Removed `add-subscriber` command.

## [2.15.0] - 2024-04-07

### Added

- Added API endpoint for manually adding subscribers.

### Changed

- Increased Monero Wallet RPC timeout to 15 seconds.
- Use actor ID or username in log messages instead of profile UUID.
- Accept objects where `inReplyTo` field contains embedded object.
- Delete orphaned media files after post update.
- Don't create notification if subscriber was added manually.
- Apply mention policy to incoming `Update(Note)` activities.

## [2.14.0] - 2024-03-30

### Added

- Implemented FEP-ef61 resolver endpoint.
- Use actor ID as handle if acct URI is not known.
- Added `enable-fep-ef61` command.
- Support unicode emoji shortcodes.
- Process incoming `Dislike` activities as "👎" reaction.
- Added API endpoint for self-deletion.

### Changed

- Preserve wrapped database errors when converting from `AuthenticationError` to `InboxError`.
- Use resolver URLs instead of plain DID URLs in FEP-ef61 representations of objects.
- Write warning to log if `preferrredUsername` doesn't match cached value.
- Don't rely on acct comparsion when verifying activity signature.
- Use actor ID instead of webfinger address in logs.
- Use actor ID as primary identifier instead of webfinger address.
- Don't publish FEP-ef61 representation if user didn't enable FEP-ef61.
- Remove colon only once when parsing emoji shortcodes.

### Removed

- Removed NFT support.

### Fixed

- Don't fill `bio_source` database column when new profile is created.
- Prevent violation of unique constraint on `acct` column when changing username.
- Remove HTML tags from post title.
- Fixed incorrect parsing of mentions containing two underscores.
- Fixed permission check that prevented users from viewing their own direct messages.

## [2.13.0] - 2024-03-12

### Added

- Support calling actor endpoint with `fep_ef61` query parameter.
- Publish FEP-ef61 representations of local objects.
- Add new notification type for emoji reactions.
- Create `add-subscriber` command.

### Changed

- Ignore `Add` activities with embedded objects.
- Add instance title to post previews generated for Matrix.
- Validate hostname length before saving profile or emoji to database.
- Ignore invalid emoji reactions.
- Use FEP-ef61 payment links in FEP-ef61 actor representation.
- Accept object attachments with type `Audio`.
- Write message to log when profile lookup by mention name is successful.
- Change media description max length to 2000.
- Perform unsolicited message check after putting activity into a queue.
- Save content of remote emoji reactions to database.
- Don't reject `Emoji` objects without `updated` property.
- Don't store chain ID if subscription has remote recipient.
- Update subscription expiration date if `Add(Person)` activity is not linked to an agreement.

### Removed

- Removed deprecated `description_source` property from Mastodon API Instance.
- Stop accepting legacy identity proofs.
- Removed `/users/<username>/fep_ef61` endpoint.

### Fixed

- Don't mark posts from remote actor as spam if pending follow request exists.
- Don't drop activity if attachment doesn't have `url` or `href` property.
- Fix error in `Add(Person)` activity handling when FEP-0837 is not used.

## [2.12.0] - 2024-02-26

### Added

- Added documentation for `update-config` command.
- Generate Ed25519 key for instance actor.
- Added "Only Contacts" mention policy.
- Publish FEP-ef61 variants of local actors.

### Changed

- Add details to mention filter log message.
- Reduce number of database queries made during mention filtering.
- Set timeout on monero-wallet-rpc requests.
- Apply custom migrations when `mitractl` is used.
- Copy instance RSA key from `instance_rsa_key` file to database.
- Enable integrity proofs with `eddsa-jcs-2022` cryptosuite by default.
- Fetch custom emojis used in `Like` activities.
- Allow underscores in hashtags.
- Don't remove conversation participants when filtering mentions.

### Fixed

- Hide notifications from muted accounts.
- Prevent multiple greentext lines from being merged into one.

## [2.11.0] - 2024-02-20

### Added

- Implemented mention filter.

### Changed

- Changed MSRV to 1.63.0.
- Added `application/ld+json` to the list of allowed object content types.
- Validate description length when updating media attachment.
- Write warning to log when direct message doesn't contain mentions of local users.

### Removed

- Removed `/api/v1/accounts/search_public` endpoint.

## [2.10.0] - 2024-02-17

### Added

- Added Mastodon API endpoint for updating media descriptions.
- Add `name` attribute to media attachment if it has description.
- Added `update-config` command.
- Added `federated_timeline_restricted` parameter to Instance object.

### Changed

- Change MSRV to 1.62.1.
- Ignore `Update(Actor)` if profile is not found locally.
- Log `content` of `Like` and `EmojiReact` activities.
- Set limit on client config size.
- Remove `charset` directive when parsing `Accept` and `Content-Type` headers.
- Accept media attachment if `Content-Type` header contains `charset` directive.
- Allow rel=tag in incoming notes.

### Removed

- Stop accepting pre-FEP-0837 proposals.

### Fixed

- Don't stop fetching replies if one item is not valid.
- Don't return "unexpected object ID" error if response in not a JSON document.
- Don't write "too many attachments" warning to log if the number is within limit.

### Security

- Verify that fetched object has AP or AS2 content type.

## [2.9.0] - 2024-02-07

### Added

- Replace `<img>` tags in posts with links.
- Generate Ed25519 keys for all accounts.
- Mark account as "bot" if remote actor has `Application` or `Service` type.
- Send notification to admin when new user is registered.

### Changed

- Change MSRV to 1.61.
- Rate-limit requests to `/api/v1/accounts/search` when `resolve` parameter is used by unauthenticated user.
- Process all incoming activities in background.
- Remove inbox mutex.
- Update FEP-0837 implementation.
- Truncate titles longer than 300 characters.

### Fixed

- Don't return error when replying to a public post with a direct message.

### Security

- Verify `Digest` header value against activity hash.

## [2.8.0] - 2024-01-29

### Added

- Add syndication feed link to user's Webfinger response.
- Support `.loki` domains in webfinger queries.
- Add flag to profile fields representing legacy identity proofs.

### Changed

- Ignore Webfinger links where `type` differs from AP/AS media types.
- Return empty search results if `offset` is not 0.
- Prevent actor ID base from changing during profile update.
- Create mentions for known remote actors in "to" and "cc" fields.
- Remove LD signature before verifying integrity proof.
- Improve error handling in mention processor.
- Rename `refetch-actor` command to `fetch-actor`.
- Always refetch target actor when processing `Move` activity.
- Change redirect limit for federation requests to 3.
- Ignore invalid attachments.
- Return error when `eddsa-jcs-2022` is used on document without `@context`.
- Use `eddsa-jcs-2022` for signing activities.

### Removed

- Removed support for payment links without `rel` attribute.

### Fixed

- Create notifications if new mentions are added to post.
- Fix validation of actor context.

### Security

- Verify that `id` and `attributedTo` have same hostname.
- Verify that `id` of fetched object matches its actual location.

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
