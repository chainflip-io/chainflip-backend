#!/bin/bash

binary_version_full=$($1 --version)
github_tag=$2

binary_semver=$(echo $binary_version_full | awk '{print $2}' | awk -F '-' '{print $1}')

echo "Extracted binary version: $binary_semver"
echo "Github tag: $github_tag"

if [ "$binary_semver" != "$github_tag" ]; then
    echo "Error: Binary version and Github tag do not match!"
    exit 1
fi
