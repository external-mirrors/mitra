# Ethereum Integration

## Sign-in with Ethereum Wallet

This feature can be enabled by adding `eip4361_ethereum` to `authentication_methods` array in Mitra configuration file:

authentication_methods:
  - password
  - eip4361_ethereum

## Verify Ethereum Keys on Profile

Click on three dots and select "Link ethereum address". The menu item will not appear if Metamask is not enabled.