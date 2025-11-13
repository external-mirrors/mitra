# Cache management

## Retention settings

Content from other servers is deleted automatically after some time (both database records and media). That time can be adjusted using configuration parameters under the `retention` key:

```yaml
retention:
  # Keep remote posts with which local accounts didn't interact for 15 days
  extraneous_posts: 15
  # Keep remote accounts without posts for 30 days
  empty_profiles: 30
```

## Manual removal

Posts:

```shell
mitra delete-extraneous-posts 15
```

Profiles:

```shell
mitra delete-empty-profiles 30
```

Delete attachments that don't belong to any post:

```shell
mitra delete-unused-attachments 5
```

## Media proxy

Caching of media can be disabled completely by adding a [federation filter](./filter.md) rule:

```shell
mitra add-filter-rule proxy-media server.example
```
