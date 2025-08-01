# ActivityPub federation in Mitra

## Supported federation protocols and standards

Mitra largely follows the [ActivityPub](https://www.w3.org/TR/activitypub/) server-to-server specification but it makes uses of some non-standard extensions, some of which are required for interacting with it.

It also supports the following standards:

- [Http Signatures](https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures)
- [NodeInfo](https://nodeinfo.diaspora.software/)
- [WebFinger](https://webfinger.net/)

## Supported FEPs

- [FEP-67ff: FEDERATION.md](https://codeberg.org/silverpill/feps/src/branch/main/67ff/fep-67ff.md)
- [FEP-f1d5: NodeInfo in Fediverse Software](https://codeberg.org/fediverse/fep/src/branch/main/fep/f1d5/fep-f1d5.md)
- [FEP-e232: Object Links](https://codeberg.org/silverpill/feps/src/branch/main/e232/fep-e232.md)
- [FEP-0ea0: Payment Links](https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md)
- [FEP-fb2a: Actor metadata](https://codeberg.org/fediverse/fep/src/branch/main/fep/fb2a/fep-fb2a.md)
  - Only for remote actors.
- [FEP-521a: Representing actor's public keys](https://codeberg.org/silverpill/feps/src/branch/main/521a/fep-521a.md)
- [FEP-8b32: Object Integrity Proofs](https://codeberg.org/silverpill/feps/src/branch/main/8b32/fep-8b32.md)
- [FEP-c390: Identity Proofs](https://codeberg.org/silverpill/feps/src/branch/main/c390/fep-c390.md)
- [FEP-0837: Federated Marketplace](https://codeberg.org/silverpill/feps/src/branch/main/0837/fep-0837.md)
- [FEP-03c1: Actors without acct-URI](https://codeberg.org/fediverse/fep/src/branch/main/fep/03c1/fep-03c1.md)
- [FEP-7628: Move actor](https://codeberg.org/fediverse/fep/src/branch/main/fep/7628/fep-7628.md)
- [FEP-fe34: Origin-based security model](https://codeberg.org/fediverse/fep/src/branch/main/fep/fe34/fep-fe34.md)
- [FEP-d556: Server-Level Actor Discovery Using WebFinger](https://codeberg.org/fediverse/fep/src/branch/main/fep/d556/fep-d556.md)
- [FEP-9098: Custom emojis](https://codeberg.org/fediverse/fep/src/branch/main/fep/9098/fep-9098.md)
- [FEP-c0e0: Emoji reactions](https://codeberg.org/fediverse/fep/src/branch/main/fep/c0e0/fep-c0e0.md)
  - `Like` with `content` activity is used.
- [FEP-ef61: Portable Objects](https://codeberg.org/fediverse/fep/src/branch/main/fep/ef61/fep-ef61.md)
  - Supports portable actors hosted on remote servers and portable actors registered using [FEP-ae97 clients](#fep-ae97-c2s-api).
  - Only `did:key` identities are supported. Planned support for `did:web`.
- [FEP-ae97: Client-side activity signing](https://codeberg.org/silverpill/feps/src/branch/main/ae97/fep-ae97.md)
- [FEP-1b12: Group federation](https://codeberg.org/fediverse/fep/src/branch/main/fep/1b12/fep-1b12.md)
  - Can consume `Announce(Activity)` activities, but doesn't publish them.
- [FEP-171b: Conversation Containers](https://codeberg.org/fediverse/fep/src/branch/main/fep/171b/fep-171b.md)
  - Can consume `Add(Activity)` activities.
  - Publishes `Add(Create(Note))` activities in followers-only and subscribers-only conversations.
- [FEP-9967: Polls](https://codeberg.org/fediverse/fep/src/branch/main/fep/9967/fep-9967.md)
- [FEP-f228: Backfilling conversations](https://codeberg.org/silverpill/feps/src/branch/main/f228/fep-f228.md)
  - Publishes collection of posts.
  - Can consume `context` and `contextHistory`.
- [FEP-3b86: Activity Intents](https://codeberg.org/fediverse/fep/src/branch/main/fep/3b86/fep-3b86.md)
  - Only `Object` intent is supported.
- [FEP-844e: Capability discovery](https://codeberg.org/silverpill/feps/src/branch/main/844e/fep-844e.md)
  - The `implements` property is used to signal RFC-9421 support.
- [FEP-044f: Consent-respecting quote posts](https://codeberg.org/fediverse/fep/src/branch/main/fep/044f/fep-044f.md)
  - "Consent-respecting" quotes are processed in the same way as regular quotes.

### FEPs that might be supported in the future

- [FEP-8fcf: Followers collection synchronization across servers](https://codeberg.org/fediverse/fep/src/branch/main/fep/8fcf/fep-8fcf.md)
- [FEP-7502: Limiting visibility to authenticated actors](https://codeberg.org/fediverse/fep/src/branch/main/fep/7502/fep-7502.md)
- [FEP-0499: Delivering to multiple inboxes with a multibox endpoint](https://codeberg.org/fediverse/fep/src/branch/main/fep/0499/fep-0499.md)
- [FEP-c180: Problem Details for ActivityPub](https://codeberg.org/fediverse/fep/src/branch/main/fep/c180/fep-c180.md)

## ActivityPub

The following activities and object types are supported:

- `Follow(Actor)`, `Accept(Follow)`, `Reject(Follow)`, `Undo(Follow)`.
- `Create(Note)`, `Update(Note)`, `Delete(Note)`.
- `Add(Note, target: featured)`, `Remove(Note, target: featured)`.
- `Like()`, `EmojiReact()`, `Dislike()`, `Undo(Like)`.
- `Announce(Note)`, `Undo(Announce)`.
- `Update(Actor)`, `Move(Actor)`, `Delete(Actor)`.
- `Offer(Agreement)`, `Accept(Agreement)`.
- `Add(Actor, target: subscribers)`, `Remove(Actor, target: subscribers)`.
- `Announce(Create | Update | Delete | Like | Dislike)`.
- `Add(Create | Update | Delete | Like | Dislike)`.

Activities are implemented in way that is compatible with Pleroma, Mastodon and other popular ActivityPub servers.

Objects with type other than `Note` are converted and stored in the same way as `Note` objects.

### Notable differences

- No shared inbox.
- The value of `Accept` header in outgoing requests is set to `application/ld+json; profile="https://www.w3.org/ns/activitystreams"`, [as required by the ActivityPub specification](https://www.w3.org/TR/activitypub/#retrieving-objects).
- The `self` link in WebFinger JRD has `application/ld+json; profile="https://www.w3.org/ns/activitystreams"` type.
- The object of `Accept(Follow)` activity is ID of the `Follow` activity.
- Replies to followers-only posts [inherit](#conversations) the audience from their parents.
- [`summary`](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-summary) is displayed as summary and not used as a "content warning".

## HTML

Most ["safe"](https://docs.rs/ammonia/latest/ammonia/struct.Builder.html#defaults) HTML tags are allowed, one exception is `<img>` tags which are transformed into links.

Microsyntaxes:

- Hashtags should have `rel="tag"` attribute or `.hashtag` class.
- Mentions should have `.mention` class.

## Conversations

The implementation of followers-only and subscribers-only conversations is based on [FEP-171b: Conversation Containers](https://codeberg.org/fediverse/fep/src/branch/main/fep/171b/fep-171b.md).

This means the audience is copied from the parent post when a reply is created. Scope widening is not allowed and incomplete conversations are not displayed.

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
    "type": "DataIntegrityProof",
    "cryptosuite": "eddsa-jcs-2022",
    "created": "2023-01-28T01:22:40.183273595Z",
    "proofPurpose": "assertionMethod",
    "proofValue": "z5djAdMSrV...",
    "verificationMethod": "https://server1.example/users/alice#ed25519-key"
  },
  "to": [
    "https://server2.example/users/bob",
    "https://www.w3.org/ns/activitystreams#Public"
  ],
  "type":"Like"
}
```

### Supported proof suites

#### eddsa-jcs-2022

https://w3c.github.io/vc-di-eddsa/#eddsa-jcs-2022

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

### FEP-7628 (pull mode)

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

<a name="fep-ae97-c2s-api"></a>
## FEP-ae97 C2S API

The `X-Invite-Code` HTTP header is required for registration of portable actors.

## Subscriptions

### Subscriber-only posts

Local actors have `subscribers` property which points to the collection of actor's paid subscribers.

Subscriber-only posts are addressed to this collection. They are also addressed to each subscriber individually, and therefore could be processed by other Fediverse services as direct messages with multiple recipients.

### Payments

Cross-instance payments are implemented according to [FEP-0837](https://codeberg.org/silverpill/feps/src/branch/main/0837/fep-0837.md) specification.

Proposals are linked to actors using [FEP-0ea0](https://codeberg.org/silverpill/feps/src/branch/main/0ea0/fep-0ea0.md) payment links. [CAIP-19](https://chainagnostic.org/CAIPs/caip-19) asset IDs are used to specify currencies.

Agreements contain a FEP-0ea0 payment link pointing to [CAIP-10](https://chainagnostic.org/CAIPs/caip-10) account ID.

### Subscription events

The `Add` activity is used to notify subscribers about their status (e.g. after successful subscription payment). Upon receipt of this activity, the receiving server should add the actor specified in the `object` property to sender's `subscribers` collection (specified in the `target` property):

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

The `Remove` activity is used to notify subscribers about expired subscriptions. Upon receipt of this activity, the receiving server should remove the actor specified in the `object` property from sender's `subscribers` collection (specified in the `target` property):

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

## Mitra Web client

### Cryptocurrency addresses in profile

`PropertyValue` attachments where `name` attribute is a currency symbol prefixed with `$` are recognized as cryptocurrency addresses:

```json
{
  "name": "$XMR",
  "type": "PropertyValue",
  "value": "8Ahza5RM4JQgtdqvpcF1U628NN5Q87eryXQad3Fy581YWTZU8o3EMbtScuioQZSkyNNEEE1Lkj2cSbG4VnVYCW5L1N4os5p"
}
```

Some commonly used labels like `LUD16` are recognized as well.
