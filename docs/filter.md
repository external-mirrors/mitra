# Federation filter

Federation filter is a firewall-like system for server-wide moderation. It operates on the network level and doesn't generate any ActivityPub messages.

A filter rule consists of an action and a target. Supported actions:

- `reject`: reject all incoming messages.
- `accept`: accept all incoming messages.
- `reject-media-attachments`: remove media attachments from posts.
- `accept-media-attachments`: allow media attachments.
- `reject-profile-images`: remove profile images.
- `accept-profile-images`: allow profile images.
- `reject-custom-emojis`: remove custom emojis from posts and profile descriptions.
- `accept-custom-emojis`: allow custom emojis.

Target is a domain name or a wildcard pattern (e.g. `*.example.com`).

Wildcard rules are applied first. For example, a ruleset for allowlist-based federation might look like this:

```
reject *
accept server1.example
accept server2.example
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
