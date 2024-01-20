# ActivityPub federation in Mitra

Mitra largely follows the [ActivityPub](https://www.w3.org/TR/activitypub/) server-to-server specification but it makes uses of some non-standard extensions, some of which are required for interacting with it.

The following activities and object types are supported:

- `Follow(Actor)`, `Accept(Follow)`, `Reject(Follow)`, `Undo(Follow)`.
- `Create(Note)`, `Update(Note)`, `Delete(Note)`.
- `Like()`, `Undo(Like)`.
- `Announce(Note)`, `Undo(Announce)`.
- `Update(Actor)`, `Move(Actor)`, `Delete(Actor)`.
- `Offer(Agreement)`, `Accept(Agreement)`.
- `Add(Actor)`, `Remove(Actor)`.

`Article`, `Event`, `Question`, `Page` and `Video` object types are partially supported.

And these additional standards:

- [Http Signatures](https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures)
- [NodeInfo](https://nodeinfo.diaspora.software/)
- [WebFinger](https://webfinger.net/)

Activities are implemented in way that is compatible with Pleroma, Mastodon and other popular ActivityPub servers.

Supported FEPs:

- [FEP-67ff: FEDERATION.md](https://codeberg.org/silverpill/feps/src/branch/main/67ff/fep-67ff.md)
- [FEP-f1d5: NodeInfo in Fediverse Software](https://codeberg.org/fediverse/fep/src/branch/main/fep/f1d5/fep-f1d5.md)
- [FEP-e232: Object Links](https://codeberg.org/silverpill/feps/src/branch/main/e232/fep-e232.md)
- [FEP-8b32: Object Integrity Proofs](https://codeberg.org/silverpill/feps/src/branch/main/8b32/fep-8b32.md)
- [FEP-0ea0: Payment Links](https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md)
- [FEP-521a: Representing actor's public keys](https://codeberg.org/silverpill/feps/src/branch/main/521a/fep-521a.md)
- [FEP-c390: Identity Proofs](https://codeberg.org/silverpill/feps/src/branch/main/c390/fep-c390.md)
- [FEP-0837: Federated Marketplace](https://codeberg.org/silverpill/feps/src/branch/main/0837/fep-0837.md)

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

A variant of [eddsa-jcs-2022](https://w3c.github.io/vc-di-eddsa/#eddsa-jcs-2022) cryptosuite without context injection.

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

## Identity proofs

Supported proof types:

- `jcs-eddsa-2022`: A variant of [eddsa-jcs-2022](https://w3c.github.io/vc-di-eddsa/#eddsa-jcs-2022) cryptosuite without context injection.
- `MitraJcsEip191Signature2022`: EIP-191 (Ethereum personal signatures)
- `MitraJcsEd25519Signature2022`: [Minisign](https://jedisct1.github.io/minisign/) (pre-hashed)

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

Where `object` is an ID of old account and `target` is an ID of new account. Actors identified by `object` and `target` properties must have at least one FEP-c390 identity in common to be considered aliases. Upon receipt of such activity, actors that follow `object` should un-follow it and follow `target` instead.

## Subscriptions

Cross-instance payments are implemented according to [FEP-0837](https://codeberg.org/silverpill/feps/src/branch/main/0837/fep-0837.md) specification.

Proposals are linked to actors using [FEP-0ea0](https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md) payment links. [CAIP-19](https://chainagnostic.org/CAIPs/caip-19) asset IDs are used to specify currencies.

Agreements contain a FEP-0ea0 payment link pointing to [CAIP-10](https://chainagnostic.org/CAIPs/caip-10) account ID.

### Subscription events

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
  "startTime": null,
  "endTime": "2023-08-30T18:15:20.765206474Z",
  "context": "https://server.example/objects/018a47a8-35bd-7bd2-b2d2-2f40b628d9b7",
  "to": [
    "https://server.example/users/bob"
  ],
  "type": "Add"
}
```

The `endTime` property specifies the subscription expiration time.

The `context` property contains a reference to an `Agreement` object.

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

## Cryptocurrency addresses in profile

`PropertyValue` attachments where `name` attribute is a currency symbol prefixed with `$` are recognized as cryptocurrency addresses:

```json
{
  "name": "$XMR",
  "type": "PropertyValue",
  "value": "8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p"
}
```

Some commonly used labels like `LUD16` are recognized as well.
