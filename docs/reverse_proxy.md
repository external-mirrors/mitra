# Reverse proxy guide

## Caddy

[Caddy](https://caddyserver.com/) is a simple to use reverse proxy system that works well.

Here is a reasonable starting [Caddyfile](https://caddyserver.com/docs/caddyfile)  which should score an A+ on [Qualys SSL Labs](https://www.ssllabs.com/ssltest/)

#### /etc/caddy/Caddyfile: 

````
{
    # Change to your email address for ssl certificate generation
	email youremail@yourdomain.com 

    # We are enabling http/3 although it's not required at this time it can contribute to a A+ rating
	servers :443 {
		protocols h1 h2 h3
	}

    # Let's setup reasonable logging defaults and target location
	log {
		level debug
		output file /var/log/caddy/caddy.log {
			roll_size 10mb
			roll_keep 5
			roll_keep_for 48h
		}
	}

yourdomain.tld {
    # Enable response body compression
	encode zstd gzip

    # Set extra header information when using Tor. This requires a V3 onion address to be available (optional)
	header {
		Onion-Location sd84tkgetmbqayrl3kmgxe7fltn6tzkhw5wumwnadztlm5s44j2dkyd.onion{uri}
	}
    
    # Set HSTS headers to improve security posture 
	header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload"

    # Set request body size maximum (This conincides with attachment size. Increase or decrease as you deem necessary)
	request_body {
		max_size 1MB
	}

    # Explicit reverse proxy with passing the origin host header to the backend properly
	reverse_proxy 127.0.0.1:8383 {
		header_up Host {http.request.host}
	}

    # Explicit declaration of tls protocols.
	tls {
		protocols tls1.2 tls1.3
	}
}
````
