# Federation filter

Federation filter is a firewall-like system for server-wide moderation. It operates on the network level and doesn't generate any ActivityPub messages.

A filter rule consists of an action and a target. Supported actions:

- `reject`: reject incoming messages only.
- `accept`: accept incoming messages.
- `reject-data`: reject all profiles and posts, block deliveries.
- `accept-data`: accept profiles and posts.
- `reject-media-attachments`: remove media attachments from posts.
- `accept-media-attachments`: allow media attachments.
- `reject-profile-images`: remove profile images.
- `accept-profile-images`: allow profile images.
- `reject-custom-emojis`: remove custom emojis from posts and profile descriptions.
- `accept-custom-emojis`: allow custom emojis.
- `mark-sensitive`: mark media attachments as sensitive.
- `reject-keywords`: reject posts containing selected keywords.
- `accept-keywords`: accept posts containing selected keywords.

Target is a domain name or a wildcard pattern (e.g. `*.example.com`).

Wildcard rules are applied last. For example, a ruleset for allowlist-based federation might look like this:

```
accept server1.example
accept server2.example
reject *
```

## Commands

List rules:

```shell
mitractl list-filter-rules
```

Add rule:

```shell
mitractl add-filter-rule reject mastodon.social
```

Remove rule:

```shell
mitractl remove-filter-rule reject mastodon.social
```

Use `update-config` command to change the list of filtered keywords:

```shell
mitractl update-config filter_keywords '["foo", "bar"]'
```
