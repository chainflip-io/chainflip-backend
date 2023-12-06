#!/bin/bash

# Check if one argument is provided
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 [version]"
    exit 1
fi

# Define the base directory
base_directory="engine/package"
cargo_file="engine/Cargo.toml"

# List of subdirectories to check
subdirectories=("berghain" "sisyphos" "perseverance")

# Get version from argument and split into major and minor
version=$1
IFS='.' read -r major_version minor_version patch_version <<< "$version"

# Construct the filename to search for
service_file="chainflip-engine${major_version}.${minor_version}.service"
search_string="chainflip-engine${major_version}.${minor_version}"

# Check each subdirectory for the specified file and content
for subdir in "${subdirectories[@]}"; do
    filepath="$base_directory/$subdir/$filename"
    if [[ -f "$filepath" ]]; then
        echo "File exists: $filepath"

        # Check if the file contains the string 'chainflip-engine1.1'
        if grep -q "chainflip-engine${major_version}.${minor_version}" "$filepath"; then
            echo "The string '$search_string' exists in $filepath"
        else
            echo "The string '$search_string' does not exist in $filepath"
            exit 2
        fi
    else
        echo "File missing: $filepath"
        exit 2
    fi
done

# Check if the string exists in engine/cargo.toml
if [[ -f "$cargo_file" ]]; then
    if grep -q "usr/bin/$search_string" $cargo_file; then
        echo "The string '$search_string' exists in $cargo_file"
    else
        echo "The string '$search_string' does not exist in $cargo_file"
        exit 2
    fi
else
    echo "The file $cargo_file does not exist"
    exit 2
fi
