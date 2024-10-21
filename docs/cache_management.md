# Cache management

Content from other servers is deleted automatically after some time (both database records and media). That time can be adjusted using configuration parameters under the `retention` key:

```yaml
retention:
  # Remote posts with which local accounts didn't interact
  extraneous_posts: 15
  # Remote accounts without posts
  empty_profiles: 60
```

## Manual removal

Posts:

```shell
mitractl delete-extraneous-posts 15
```

Profiles:

```shell
mitractl delete-empty-profiles 60
```

Delete attachments that don't belong to any post:

```shell
mitractl delete-unused-attachments 5
```

Delete unused remote emojis:

```shell
mitractl prune-remote-emojis
```
