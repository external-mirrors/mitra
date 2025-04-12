# Relays

Mitra only supports LitePub relay protocol. Mastodon relay protocol is not supported.

## Following a relay

Create a new user:

```
mitra create-account followbot <password>
```

Log in, search for a relay actor and follow it.

Posts announced by relay actor will appear in the federated timeline. The relay actor will also follow the user. Any reposts made by the user will be broadcasted by the relay.

---

Pleroma instances may have a relay actor at `https://server.example/relay`.
