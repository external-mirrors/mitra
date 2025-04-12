# Backup and restore

## Backup

1. Back up the database
2. Back up media. Skip remote cached media to save space (`mitra list-local-files` can be used to display the list of local media).
3. Back up the configuration file.

Example:

```shell
pg_dump --format=custom -U mitra mitra -f /opt/mitra-backup/database/mitra
su mitra -c "mitra list-local-files" | rsync -av --delete --files-from=- /var/lib/mitra/media /opt/mitra-backup/media
cp /etc/mitra/config.yaml /opt/mitra-backup/config.yaml
```

## Restore

1. Copy configuration file to Mitra configuration directory (e.g. `/etc/mitra`).
2. Copy media to `media` subdirectory of a directory specified by `storage_dir` configuration parameter.
3. Restore the database.
