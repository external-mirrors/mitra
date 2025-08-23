#!/bin/sh

# This script is deprecated, use `mitra import-posts`.

set -e

FILENAME="$1"
USERNAME="$2"
BIN='mitra'

for row in $(jq -r '.orderedItems[] | select(.type == "Create") | .object | select(.inReplyTo == null) | select(.to | index("https://www.w3.org/ns/activitystreams#Public")) | @base64' ${FILENAME}); do
    object=$(echo ${row} | base64 --decode)
    content=$(echo ${object} | jq -r .content)
    published=$(echo ${object} | jq -r .published)
    ${BIN} create-post ${USERNAME} "${content}" ${published}
done
