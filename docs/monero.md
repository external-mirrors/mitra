# Monero integration

## Payments

Monero payments can be tracked using two different methods:

- Forwarding. This method works for any kind of payout address, but relies on an intermediary wallet.
- View-only wallet. This method enables direct payments, but it requires users to supply a primary wallet address and a corresponding view key.

### Forwarding

Install a [Monero node](https://docs.getmonero.org/running-node/monerod-systemd/) (requires at least 4 GB RAM and 100 GB storage) or choose a [public one](https://monero.fail/).

Install and configure [monero-wallet-rpc](https://docs.getmonero.org/rpc-library/wallet-rpc/) service. See configuration file [example](../contrib/monero/wallet.conf).

Start `monero-wallet-rpc`.

Add blockchain configuration to the `blockchains` array in your Mitra [configuration file](../config.example.yaml).

Example:

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

### View-only wallet

Install a [Monero node](https://docs.getmonero.org/running-node/monerod-systemd/) (requires at least 4 GB RAM and 100 GB storage). Most public nodes are not suitable because they don't accept ZMQ RPC requests.

Install [Monero Light Wallet Server (LWS)](https://github.com/vtnerd/monero-lws) version 0.3. Then add blockchain configuration to the `blockchains` array in your Mitra [configuration file](../config.example.yaml):

```yaml
blockchains:
  - chain_id: monero:mainnet
    lightwallet_api_url: 'http://127.0.0.1:18443'
```

## Sign-in with Monero wallet

This feature can be enabled by adding `caip122_monero` to `authentication_methods` array in Mitra configuration file:

```yaml
authentication_methods:
  - password
  - caip122_monero
```
