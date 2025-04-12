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
mitra update-config --help
mitra update-config <parameter> <value>
```

Generate invite code (note is optional):

```shell
mitra generate-invite-code <note>
```

List generated invites:

```shell
mitra list-invite-codes
```

Create account:

```shell
mitra create-account <username> <password> <role-name>
```

List local accounts:

```shell
mitra list-accounts
```

Set or change password:

```shell
mitra set-password <user-id-or-name> <password>
```

Change user's role (admin, user or read_only_user).

```shell
mitra set-role <user-id-or-name> <role-name>
```

Delete user:

```shell
mitra delete-user 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete post:

```shell
mitra delete-post 55a3005f-f293-4168-ab70-6ab09a879679
```

Delete custom emoji:

```shell
mitra delete-emoji emoji_name example.org
```

Add custom emoji to local collection:

```shell
mitra add-emoji emoji_name /path/to/image.png
mitra add-emoji emoji_name https://social.example/path/to/image.png
```

Import custom emoji from another instance:

```shell
mitra import-emoji emoji_name example.org
```

Generate instance report:

```shell
mitra instance-report
```
