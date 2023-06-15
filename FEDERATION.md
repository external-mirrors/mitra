# ActivityPub federation in Mitra

Mitra largely follows the [ActivityPub](https://www.w3.org/TR/activitypub/) server-to-server specification but it makes uses of some non-standard extensions, some of which are required for interacting with it.

The following activities and object types are supported:

- `Follow(Actor)`, `Accept(Follow)`, `Reject(Follow)`, `Undo(Follow)`.
- `Create(Note)`, `Update(Note)`, `Delete(Note)`.
- `Like()`, `Undo(Like)`.
- `Announce(Note)`, `Undo(Announce)`.
- `Update(Actor)`, `Move(Actor)`, `Delete(Actor)`.
- `Add(Actor)`, `Remove(Actor)`.

`Article`, `Event`, `Question`, `Page` and `Video` object types are partially supported.

And these additional standards:

- [Http Signatures](https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures)
- [NodeInfo](https://nodeinfo.diaspora.software/)
- [WebFinger](https://webfinger.net/)

Activities are implemented in way that is compatible with Pleroma, Mastodon and other popular ActivityPub servers.

Supported FEPs:

- [FEP-f1d5: NodeInfo in Fediverse Software](https://codeberg.org/fediverse/fep/src/branch/main/fep/f1d5/fep-f1d5.md)
- [FEP-e232: Object Links](https://codeberg.org/silverpill/feps/src/branch/main/e232/fep-e232.md)
- [FEP-8b32: Object Integrity Proofs](https://codeberg.org/silverpill/feps/src/branch/main/8b32/fep-8b32.md)
- [FEP-0ea0: Payment Links](https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md)
- [FEP-521a: Representing actor's public keys](https://codeberg.org/silverpill/feps/src/branch/main/521a/fep-521a.md)

## Object integrity proofs

All outgoing activities are signed with actor's key in accordance with [FEP-8b32](https://codeberg.org/silverpill/feps/src/branch/main/8b32/fep-8b32.md) document.

Example:

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams",
    "https://w3id.org/security/data-integrity/v1"
  ],
  "actor": "https://server1.example/users/alice",
  "cc": [],
  "id": "https://server1.example/objects/0185f5f8-10b5-1b69-f45e-25f06792f411",
  "object": "https://server2.example/users/bob/posts/141892712081205472",
  "proof": {
    "created": "2023-01-28T01:22:40.183273595Z",
    "proofPurpose": "assertionMethod",
    "proofValue": "z5djAdMSrV...",
    "type": "MitraJcsRsaSignature2022",
    "verificationMethod": "https://server1.example/users/alice#main-key"
  },
  "to": [
    "https://server2.example/users/bob",
    "https://www.w3.org/ns/activitystreams#Public"
  ],
  "type":"Like"
}
```

### Supported proof suites

#### jcs-eddsa-2022

https://w3c.github.io/vc-di-eddsa/#jcs-eddsa-2022

#### MitraJcsRsaSignature2022

Canonicalization algorithm: JCS  
Hashing algorithm: SHA-256  
Signature algorithm: RSASSA-PKCS1-v1_5

#### MitraJcsEip191Signature2022

Canonicalization algorithm: JCS  
Hashing algorithm: KECCAK-256 (EIP-191)  
Signature algorithm: ECDSA (EIP-191)

#### MitraJcsEd25519Signature2022

Canonicalization algorithm: JCS  
Hashing algorithm: BLAKE2b-512  
Signature algorithm: EdDSA

## Quotes

Supported representations:

- `quoteUrl` property.
- FEP-e232 object links with relation type `https://misskey-hub.net/ns#_misskey_quote`.

## Custom emojis

Custom emojis are implemented as described in Mastodon documentation: https://docs.joinmastodon.org/spec/activitypub/#emoji.

## Profile extensions

### Cryptocurrency addresses

Cryptocurrency addresses are represented as `PropertyValue` attachments where `name` attribute is a currency symbol prefixed with `$`:

```json
{
  "name": "$XMR",
  "type": "PropertyValue",
  "value": "8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p"
}
```

### Identity proofs

Identity proofs are represented as attachments of `IdentityProof` type:

```json
{
  "name": "<did>",
  "type": "IdentityProof",
  "signatureAlgorithm": "<proof-type>",
  "signatureValue": "<proof>"
}
```

Supported proof types:

- EIP-191 (Ethereum personal signatures)
- [Minisign](https://jedisct1.github.io/minisign/)

[FEP-c390](https://codeberg.org/silverpill/feps/src/branch/main/c390/fep-c390.md) identity proofs are not supported yet.

## Account migrations

After registering an account its owner can upload the list of followers and start the migration process. The server then sends `Move` activity to each follower:

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams"
  ],
  "actor": "https://server2.example/users/alice",
  "id": "https://server2.example/activities/00000000-0000-0000-0000-000000000001",
  "object": "https://server1.example/users/alice",
  "target": "https://server2.example/users/alice",
  "to": [
    "https://server.example/users/bob"
  ],
  "type": "Move"
}
```

Where `object` is an ID of old account and `target` is an ID of new account. Actors identified by `object` and `target` properties must have at least one identity key in common to be considered aliases. Upon receipt of such activity, actors that follow `object` should un-follow it and follow `target` instead.

## Subscription events

Local actor profiles have `subscribers` property which points to the collection of actor's paid subscribers.

The `Add` activity is used to notify the subscriber about successful subscription payment. Upon receipt of this activity, the receiving server should add specified `object` to actors's `subscribers` collection (specified in `target` property):

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams"
  ],
  "actor": "https://server.example/users/alice",
  "id": "https://server.example/activities/00000000-0000-0000-0000-000000000001",
  "object": "https://server.example/users/bob",
  "target": "https://server.example/users/alice/collections/subscribers",
  "to": [
    "https://server.example/users/bob"
  ],
  "type": "Add"
}
```

The `Remove` activity is used to notify the subscriber about expired subscription. Upon receipt of this activity, the receiving server should remove specified `object` from actors's `subscribers` collection (specified in `target` property):

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams"
  ],
  "actor": "https://server.example/users/alice",
  "id": "https://server.example/activities/00000000-0000-0000-0000-000000000002",
  "object": "https://server.example/users/bob",
  "target": "https://server.example/users/alice/collections/subscribers",
  "to": [
    "https://server.example/users/bob"
  ],
  "type": "Remove"
}
```
