#!/usr/bin/env bash

# Remove any prefixes like "v" from the tag
tag="${1#v}"

# Extract major and minor version using cut command
version=$(echo "$tag" | cut -d. -f1,2)

# Output the major and minor version
echo "$version"