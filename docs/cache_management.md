# Cache management

Content from other servers is deleted automatically after some time (both database records and media). That time can be adjusted using configuration parameters under the `retention` key:

```yaml
retention:
  # Remote posts with which local accounts didn't interact
  extraneous_posts: 15
  # Remote accounts without posts
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
