# Migrations

Mitra supports 3 different migration mechanisms: migration of followers (push mode), migration of followers (pull mode) and portable accounts (aka nomadic identity).

## Migration of followers (push mode)

This type of account migration is widely supported in Fediverse. Migration is initiated by the current server.

## Migration of followers (pull mode)

This type of account migration is only supported by Mitra. It can be initiated from any server and works even if the current server is offline. It relies on identity proofs ([FEP-c390](https://codeberg.org/silverpill/feps/src/branch/main/c390/fep-c390.md)).

## Portable accounts

An implementation of [FEP-ef61](https://codeberg.org/silverpill/feps/src/branch/main/ef61/fep-ef61.md), this feature is still experimental and requires a specialized [client](../c2s.md).
