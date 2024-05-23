# Ethereum integration

Install Ethereum client or choose a JSON-RPC API provider.

Deploy contracts on the blockchain. Instructions can be found at https://codeberg.org/silverpill/mitra-contracts.

Add blockchain configuration to `blockchains` array in your configuration file:

```yaml
blockchains:
  - chain_id: eip155:31337
    chain_metadata:
      chain_name: localhost
      currency_name: ETH
      currency_symbol: ETH
      currency_decimals: 18
      public_api_url: 'http://127.0.0.1:8545'
      explorer_url: null
    contract_address: '0xDc64a140Aa3E981100a9becA4E685f962f0cF6C9'
    contract_dir: /usr/share/mitra/contracts
    api_url: 'http://127.0.0.1:8545'
    signing_key: null
    chain_sync_step: 1000
    chain_reorg_max_depth: 10
```

Chain metadata for EVM chains can be found at https://github.com/ethereum-lists/chains.

Signing key for ethereum integration can be generated with `mitractl generate-ethereum-address`.
