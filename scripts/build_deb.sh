#!/bin/sh

set -e
set -x

VERSION=$(cargo metadata --quiet --no-deps --offline | jq -r '.packages[] | select(.name == "mitra") | .version')
VERSION_DEB=$(echo $VERSION | tr "-" "~")
ARCH=$(dpkg --print-architecture)

# Package contents will appear in target/debian/tmp/debian/mitra/
PACKAGE_DIR="target/debian/tmp"
WEB_DIR="$1"

# Package info
rm -rf $PACKAGE_DIR
mkdir -p $PACKAGE_DIR/debian
cp contrib/debian/* $PACKAGE_DIR/debian/
sed -i "s/0.0.0/${VERSION_DEB}/" $PACKAGE_DIR/debian/changelog
echo "Architecture: $ARCH" >> $PACKAGE_DIR/debian/control

# Service
cp contrib/mitra.service $PACKAGE_DIR/debian/mitra.service

# Config file
mkdir -p $PACKAGE_DIR/etc/mitra
cp config.example.yaml $PACKAGE_DIR/etc/mitra/config.yaml

# Config example
mkdir -p $PACKAGE_DIR/usr/share/mitra/examples
cp config.example.yaml $PACKAGE_DIR/usr/share/mitra/examples/config.yaml

# Binaries
if [ -z "$TARGET" ]; then
    TARGET_DIR=target/release
else
    TARGET_DIR=target/$TARGET/release
fi
mkdir -p $PACKAGE_DIR/usr/bin
cp $TARGET_DIR/mitra $PACKAGE_DIR/usr/bin/mitra

# Completions
./$TARGET_DIR/mitra completion --shell bash > /usr/share/bash-completion/completions/mitra

# Webapp
mkdir -p $PACKAGE_DIR/usr/share/mitra
# https://people.debian.org/~neilm/webapps-policy/ch-issues.html#s-issues-fhs
cp -r $WEB_DIR $PACKAGE_DIR/usr/share/mitra/www

# Build
cd $PACKAGE_DIR
dpkg-buildpackage --build=binary --unsigned-changes
