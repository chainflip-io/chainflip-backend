#!/bin/bash

# Check if one argument is provided
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 [version]"
    exit 1
fi

# Changelog file path
changelog_file="CHANGELOG.md"

# Version to check for in the changelog
version=$1

# Check if the changelog file exists
if [[ ! -f "$changelog_file" ]]; then
    echo "Changelog file not found: $changelog_file"
    exit 2
fi

# Search for the version in the changelog file
if grep -q "$version" "$changelog_file"; then
    echo "Version $version found in the changelog."
else
    echo "Version $version not found in the changelog."
    exit 3
fi
