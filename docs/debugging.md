# Debugging

## Log

If you run Mitra as a systemd service, the log can be viewed using `journalctl`:

```shell
journalctl -u mitra
```

Example of a log message:

```
2025-10-03T17:20:57 mitra_adapters::init [INFO] config loaded from /etc/mitra/config.yaml
```

The value in square brackets indicates the severity:

- **ERROR**: serious errors
- **WARN**: minor errors and warnings
- **INFO**: informational messages
- **DEBUG**: low-lovel debug messages (not displayed by default)

## Instance report

Instance report can be generated using the `instance-report` command:

```
mitra instance-report
```

## Metrics

The [OpenMetrics](https://prometheus.io/docs/specs/om/open_metrics_spec/) API endpoint is located at `/metrics` path.

This endpoint is disabled by default and can be enabled by adding the `metrics` block to the configuration file:

```yaml
metrics:
  auth_username: username
  auth_password: passw0rd
```

