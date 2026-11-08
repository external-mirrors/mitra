# Command line interface

Commands must be run as the same user as the web service:

```shell
su mitra -s $SHELL -c "mitra instance-report"
```

## Basic commands

Print version:

```shell
mitra --version
```

Print help:

```shell
mitra --help
```

Update dynamic configuration:

```shell
mitra config set --help
mitra config set <parameter> <value>
```

Generate invite code (note is optional):

```shell
mitra invite create <note>
```

List generated invites:

```shell
mitra invite list
```

Create account:

```shell
mitra account create <username> <password> <role-name>
```

List local accounts:

```shell
mitra account list
```

Set or change password:

```shell
mitra account password <user-id-or-name> <password>
```

Change user's role (admin, user or read_only_user).

```shell
mitra account role <user-id-or-name> <role-name>
```

Delete user:

```shell
mitra user delete 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete post:

```shell
mitra post delete 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete custom emoji:

```shell
mitra emoji delete emoji_name example.org
```

Add custom emoji to local collection:

```shell
mitra emoji create emoji_name /path/to/image.png
mitra emoji create emoji_name https://social.example/path/to/image.png
```

Copy cached custom emoji to local collection:

```shell
mitra emoji copy emoji_name example.org
```

List [federation filter](./filter.md) rules:

```shell
mitra filter list
```

Generate instance report:

```shell
mitra status
```
