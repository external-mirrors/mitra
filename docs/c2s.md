# ActivityPub Client API

[FEP-ae97](https://codeberg.org/fediverse/fep/src/branch/main/fep/ae97/fep-ae97.md) clients can register and publish activities.

This API is disabled by default and can be enabled using the `federation.fep_ef61_gateway_enabled` configuration parameter.

`X-Invite-Code` HTTP header is required for the registration of portable actors. Its value should be a code generated with `mitra generate-invite-code` command.

Supported activities:

- `Update(Actor)`
- `Create(Note)`
- `Follow(Actor)`
- `Accept(Follow)`
- `Undo(Follow)`

Items in portable inboxes and outboxes are removed after 90 days. Activities from non-portable accounts are removed after 5 days (but this time can be changed in configuration).
