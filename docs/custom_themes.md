# Custom themes

The appearance of [mitra-web](https://codeberg.org/silverpill/mitra-web) client can be changed by specifying the theme directory in `config.yaml`:

```yaml
web_client_theme_dir: /var/lib/mitra/theme
```

Files in that directory will be served instead of files in `web_client_dir` when their names match.

CSS rules can be added to [assets/custom.css](https://codeberg.org/silverpill/mitra-web/src/branch/main/public/assets/custom.css) file.
