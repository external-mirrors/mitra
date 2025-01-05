# Tor Vanity Onion Address

## Generating a Tor V3 vanity onion address

Please refer to [docs/onion](./onion.md) to get started with Tor and Mitra.

By default when the Tor service starts, it will generate a random address in our declared location which will result in `msywkvrbkaslqfnloxssiuu57x7vraisxis262tyrrouxvw3lhhmvryd.onion` or something similar. You may want to use a custom onion address to `brand` or keep within the theme of your instance.

Luckily, this is fairly straight forward. You can install the package [mkp224o](https://github.com/cathugger/mkp224o) or if you have Docker installed you can use Docker to run the container and output the key.

To run the command natively: `mkp224o -d keys -B -n 1 mitra`

To run the command via Docker: `docker run --rm -it -v $PWD:/keys ghcr.io/cathugger/mkp224o:master -d /keys -B -n 1 mitra`

This will output a directory which contains a few files:

```shell
$ ls mitra3gfhdw3uimj3okcrd2jo7uei73mwizspk3pjepcwkhcpvesomqd.onion
hostname hs_ed25519_public_key hs_ed25519_secret_key
```

These three files will need to be moved to the respective target `HiddenServiceDir` declared in our previously mentioned `/etc/tor/torrc`:

```
HiddenServiceDir /var/lib/tor/mitra/
HiddenServicePort 80 127.0.0.1:8383
```

If you've already configured `/etc/tor/torrc` and have a generated v3 onion address in that directory, you'll want to delete the directory contents of `/var/lib/tor/mitra` and replace it with the set you generated. Be sure to restart the Tor service and add the new address as per [our guide](./onion.md).
