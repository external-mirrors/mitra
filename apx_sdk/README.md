# APx

Minimalistic ActivityPub toolkit written in Rust.

Features:

- Networking.
- Authentication (HTTP signatures, object integrity proofs).
- WebFinger.
- Nomadic identity.

Using in a Cargo Project:

```toml
[dependencies]
apx_sdk = { git = "https://codeberg.org/silverpill/mitra", rev = "apx-v0.4.0" }
```

Examples:

- [FEP-ae97 client](./examples/fep_ae97_client.rs)

Used by:

- [Mitra](https://codeberg.org/silverpill/mitra)
- [Activity Connect](https://codeberg.org/silverpill/activity-connect)
- [fep-ae97-client](https://codeberg.org/silverpill/fep-ae97-client)

License: AGPL-3.0
