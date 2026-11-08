# Federation filter

Federation filter is a firewall-like system for server-wide moderation. It operates on the network level and doesn't generate any ActivityPub messages.

A filter rule consists of an action and a target. Supported actions:

- `reject-incoming`: reject incoming messages only.
- `accept-incoming`: accept incoming messages.
- `reject`: reject all profiles and posts, block deliveries.
- `accept`: accept profiles and posts.
- `reject-media-attachments`: remove media attachments from posts.
- `accept-media-attachments`: allow media attachments.
- `reject-profile-images`: remove profile images.
- `accept-profile-images`: allow profile images.
- `reject-custom-emojis`: remove custom emojis from posts and profile descriptions.
- `accept-custom-emojis`: allow custom emojis.
- `mark-sensitive`: mark media attachments as sensitive.
- `proxy-media`: use media proxy (do not download remote media).
- `cache-media`: don't use media proxy.
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
mitra filter list
```

Add rule:

```shell
mitra filter add reject mastodon.social
```

Add wildcard rule:

```shell
mitra filter add reject '*'
```

Remove rule:

```shell
mitra filter remove reject mastodon.social
```

Use `config set` command to change the list of filtered keywords:

```shell
mitra config set filter_keywords '["foo", "bar"]'
```
