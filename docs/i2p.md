# I2P federation

## Clearnet + I2P

Clearnet instances can federate with I2P-only instances.

Add the following block to Mitra configuration file:

```yaml
federation:
  i2p_proxy_url: 'socks5h://127.0.0.1:4447'
```

Where `127.0.0.1:4447` is the address and the port where I2P SOCKS proxy is listening.
