# Tor federation

Mitra can be configured to work in Tor-only mode, or in clearnet mode with Tor connection.

- **Tor-only**: federates with other instances on Tor network and with a small subset of clearnet instances that are connected to Tor.
- **Clearnet + Tor**: a regular clearnet instance that also federates with Tor-only instances.

## Tor-only instance

Install Tor. Uncomment or add the following block to Mitra configuration file:

```yaml
federation:
  proxy_url: 'socks5h://127.0.0.1:9050'
```

Where `127.0.0.1:9050` is the address and the port where Tor proxy is listening.

Configure the onion service by adding these lines to `torrc` configuration file:

```
HiddenServiceDir /var/lib/tor/mitra/
HiddenServicePort 80 127.0.0.1:8383
```

Where `8383` should correspond to `http_port` setting in Mitra configuration file.

Restart the Tor service. Inside the `HiddenServiceDir` directory find the `hostname` file. This file contains the hostname of your onion service. Change the value of `instance_uri` parameter in Mitra configuration file to that hostname (it should end with `.onion`).

Start Mitra.

An HTTP server (e.g. nginx) is not necessary in this setup. For more information about running onion services, visit https://community.torproject.org/onion-services/setup/

You can also generate a [vanity address](./onion-vanity-address.md) for your onion service.

## Clearnet + Tor

To enable Tor federation on a clearnet Mitra instance, add the following block to the configuration file:

```yaml
federation:
  onion_proxy_url: 'socks5h://127.0.0.1:9050'
```

Where `127.0.0.1:9050` is the address and the port where Tor proxy is listening.
