#!/bin/bash

version="$1"  # Version to be updated to

# Array of hardcoded file paths
files=(
  "api/bin/chainflip-broker-api/Cargo.toml"
  "api/bin/chainflip-cli/Cargo.toml"
  "api/bin/chainflip-lp-api/Cargo.toml"
  "engine/Cargo.toml"
  "state-chain/node/Cargo.toml"
  "state-chain/runtime/Cargo.toml"
)

# Loop over each file path
for file in "${files[@]}"; do
  if [[ -f $file ]]; then  # If the file exists
    # Use sed to replace the version in the file
    sed -i "s/version = \"[0-9]*\.[0-9]*\.[0-9]*\"/version = \"$version\"/g" "$file"
    echo "Updated $file"
  else
    echo "File $file does not exist"
  fi
done
