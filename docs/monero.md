# Monero integration

## Payments

Install Monero node or choose a [public one](https://monero.fail/).

Install and configure [monero-wallet-rpc](https://monerodocs.org/interacting/monero-wallet-rpc-reference/) service. See configuration file [example](../contrib/monero/wallet.conf).

Start `monero-wallet-rpc`. Create a wallet for your instance:

```
mitractl create-monero-wallet "mitra-wallet" "passw0rd"
```

Add blockchain configuration to `blockchains` array in your configuration file.

## Sign-in with Monero wallet

This feature can be enabled by adding `caip122_monero` to `authentication_methods` array in Mitra configuration file:

```yaml
authentication_methods:
  - password
  - caip122_monero
```
