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

# Construct the service file to search for
service_file="chainflip-engine${major_version}.${minor_version}.service"
engine_binary_name="chainflip-engine${major_version}.${minor_version}"

# Check each subdirectory for the specified file and content
for subdir in "${subdirectories[@]}"; do
    filepath="$base_directory/$subdir/$service_file"
    if [[ -f "$filepath" ]]; then
        echo "File exists: $filepath"

        # Check if the file contains the string 'chainflip-engine1.2'
        if grep -q "chainflip-engine${major_version}.${minor_version}" "$filepath"; then
            echo "The string '$engine_binary_name' exists in $filepath"
        else
            echo "The string '$engine_binary_name' does not exist in $filepath"
            exit 2
        fi
    else
        echo "File missing: $filepath"
        exit 2
    fi
done

# Check if the string exists in engine/cargo.toml
if [[ -f "$cargo_file" ]]; then
    if grep -q "usr/bin/$engine_binary_name" $cargo_file; then
        echo "The string '$engine_binary_name' exists in $cargo_file"
    else
        echo "The string '$engine_binary_name' does not exist in $cargo_file"
        exit 2
    fi
else
    echo "The file $cargo_file does not exist"
    exit 2
fi
