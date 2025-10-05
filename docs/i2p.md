# I2P federation

Mitra can be configured to work in I2P-only mode, or in clearnet mode with I2P connection.

- **I2P-only**: federates with other instances on I2P network and with a small subset of clearnet instances that are connected to I2P.
- **Clearnet + I2P**: a regular clearnet instance that also federates with I2P-only instances.

## I2P-only instance

Install I2P (e.g. [i2pd](https://i2pd.xyz/)).

Add the following block to the Mitra [configuration file](../config.example.yaml):

```yaml
federation:
  proxy_url: 'socks5h://127.0.0.1:4447'
```

Where `127.0.0.1:4447` is the address and the port where I2P SOCKS proxy is listening.

Configure your I2P node to create a HTTP server tunnel to your Mitra server (it listens at `127.0.0.1:8383` by default). Enable outproxy if you want to communicate with clearnet instances.

Example `tunnels.conf` entry for i2pd:

```toml
[mitra]
type=server
host=127.0.0.1
inport=80
port=8383
keys=mitra.dat
```
An HTTP server (e.g. nginx) is not necessary in this setup.

## Clearnet + I2P

Add the following block to Mitra configuration file:

```yaml
federation:
  i2p_proxy_url: 'socks5h://127.0.0.1:4447'
```

Where `127.0.0.1:4447` is the address and the port where I2P SOCKS proxy is listening.
