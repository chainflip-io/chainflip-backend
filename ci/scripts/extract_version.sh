#!/usr/bin/env bash

# Input string
input_string="$1"
# Version type to extract. default is major_minor

version_type="${2:-major_minor}"

# Regular expression to match version pattern (X.Y.Z or X.Y)
# This pattern looks for numbers separated by dots.
# The -E flag is used for extended regex, and -o to output only the matched part.
version=$(echo "$input_string" | grep -Eo '[0-9]+\.[0-9]+(\.[0-9]+)?')

# Check if a version was found
if [[ -z "$version" ]]; then
   exit 0
fi

# Extract major and minor version
major_minor=$(echo "$version" | cut -d. -f1,2)
full_version=$(echo "$version" | cut -d. -f1,2,3)

if [[ "$version_type" == "major_minor" ]]; then
    echo $major_minor
else
    echo $full_version
fi
