# Backup and restore

## Backup

1. Back up the database
2. Back up media. Skip remote cached media to save space (`mitractl list-local-files` can be used to display the list of local media).
3. Back up the configuration file.

## Restore

1. Copy configuration file to Mitra configuration directory (e.g. `/etc/mitra`).
2. Copy media to `media` subdirectory of a directory specified by `storage_dir` configuration parameter.
3. Restore the database.
