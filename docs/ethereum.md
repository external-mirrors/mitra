# Ethereum Integration

## Sign-in with Ethereum Wallet

This feature can be enabled by adding `eip4361` to `authentication_methods` array in Mitra configuration file:

```yaml
authentication_methods:
  - password
  - eip4361
```

## Verify Ethereum Keys on Profile

Click on three dots and select "Link ethereum address". The menu item will not appear if Metamask is not enabled.
