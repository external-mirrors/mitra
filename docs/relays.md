# Relays

Mitra only supports LitePub relay protocol. Mastodon relay protocol is not supported.

## Following a relay

Create a new read-only user:

```
mitractl create-account followbot <password> read_only_user
```

Log in, search for a relay actor and follow it. Posts announced by relay actor will appear in the federated timeline.

---

Pleroma instances may have a relay actor at `https://server.example/relay`.
