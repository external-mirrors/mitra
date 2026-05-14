# Devuan/Debian with Sysvinit

This script is for Devuan/Debian using sysvinit as the init system instead of systemd.

## Installation

Copy the sysvinit script: 

``` sh
cp mitra /etc/init.d
```

Add execute permission:

``` sh
chmod +x /etc/init.d/mitra
```

## Usage

Enable service:

``` sh
update-rc.d mitra defaults
```

Disable service:

``` sh
update-rc.d mitra remove
```

Start mitra:

``` sh
/etc/init.d/mitra start
```

Stop mitra:

``` sh
/etc/init.d/mitra stop
```

Restart mitra:

``` sh
/etc/init.d/mitra restart
```

Get mitra status:

``` sh
/etc/init.d/mitra status
```
