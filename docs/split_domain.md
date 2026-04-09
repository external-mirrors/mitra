# Split-domain deployments

Add `webfinger_hostname` parameter to your configuration file:

```yaml
instance_url: "https://mitra.social.example"
webfinger_hostname: "social.example"
```

Then configure your HTTP server to redirect WebFinger queries from `webfinger_hostname` to your instance.

Nginx example:

```
server {
    server_name social.example;

    location = /.well-known/webfinger {
        return 301 https://mitra.social.example$request_uri;
    }
}
```
