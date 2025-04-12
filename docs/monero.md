# Monero integration

## Payments

Install a [Monero node](https://www.getmonero.org/resources/user-guides/vps_run_node.html) (requires at least 2 GB RAM and 200 GB storage) or choose a [public one](https://monero.fail/).

Install and configure [monero-wallet-rpc](https://www.getmonero.org/resources/developer-guides/wallet-rpc.html) service. See configuration file [example](../contrib/monero/wallet.conf).

Start `monero-wallet-rpc`.

Add blockchain configuration to `blockchains` array in your configuration file. Example:

```yaml
blockchains:
  - chain_id: monero:mainnet
    wallet_rpc_url: 'http://127.0.0.1:18083'
```

Create a wallet for your instance:

```
mitra create-monero-wallet "mitra-wallet" "passw0rd"
```

Set `wallet_name` and `wallet_password` parameters in your configuration:

```yaml
blockchains:
  - chain_id: monero:mainnet
    wallet_rpc_url: 'http://127.0.0.1:18083'
    wallet_name: "mitra-wallet"
    wallet_password: "passw0rd"
```

## Sign-in with Monero wallet

This feature can be enabled by adding `caip122_monero` to `authentication_methods` array in Mitra configuration file:

```yaml
authentication_methods:
  - password
  - caip122_monero
```
