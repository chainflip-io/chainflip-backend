#!/usr/bin/env bash

# Input string
input_string="$1"

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

# Output the major and minor version
echo "$major_minor"
