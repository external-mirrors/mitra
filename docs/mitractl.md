# mitractl: a tool for instance administrators

Commands must be run as the same user as the web service:

```shell
su mitra -s $SHELL -c "mitractl instance-report"
```

Default config file path is `/etc/mitra/config.yaml`, but it can be changed using `CONFIG_PATH` environment variable.

---

Print version:

```shell
mitractl --version
```

Print help:

```shell
mitractl --help
```

Update dynamic configuration:

```shell
mitractl update-config --help
mitractl update-config <parameter> <value>
```

Generate invite code (note is optional):

```shell
mitractl generate-invite-code <note>
```

List generated invites:

```shell
mitractl list-invite-codes
```

Create user:

```shell
mitractl create-user <username> <password> <role-name>
```

List local users:

```shell
mitractl list-users
```

Set or change password:

```shell
mitractl set-password <user-id-or-name> <password>
```

Change user's role (admin, user or read_only_user).

```shell
mitractl set-role <user-id-or-name> <role-name>
```

Delete profile:

```shell
mitractl delete-profile 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete post:

```shell
mitractl delete-post 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete custom emoji:

```shell
mitractl delete-emoji emoji_name example.org
```

Remove remote posts and media older than 30 days:

```shell
mitractl delete-extraneous-posts 30
```

Delete attachments that don't belong to any post:

```shell
mitractl delete-unused-attachments 5
```

Delete empty remote profiles:

```shell
mitractl delete-empty-profiles 100
```

Delete unused remote emojis:

```shell
mitractl prune-remote-emojis
```

Add custom emoji to local collection:

```shell
mitractl add-emoji emoji_name /path/to/image.png
mitractl add-emoji emoji_name https://social.example/path/to/image.png
```

Import custom emoji from another instance:

```shell
mitractl import-emoji emoji_name example.org
```

Create Monero wallet:

```shell
mitractl create-monero-wallet "mitra-wallet" "passw0rd"
```

Check expired invoice:

```shell
mitractl check-expired-invoice 0184b062-d8d5-cbf1-a71b-6d1aafbae2ab
```

Generate instance report:

```shell
mitractl instance-report
```
