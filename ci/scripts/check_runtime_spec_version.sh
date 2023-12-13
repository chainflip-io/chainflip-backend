#!/bin/bash

version=$1
runtime_spec_version_file="state-chain/runtime/src/lib.rs"
numeric_version=$(echo $version | tr -d '.')

# Extract the spec_version from the Rust file
spec_version=$(grep 'spec_version:' $runtime_spec_version_file | awk '{print $2}' | tr -d ',')

runtime_version=$(cargo read-manifest --manifest-path state-chain/runtime/Cargo.toml | jq -r .version)

# Compare versions
if [ "$spec_version" != "$numeric_version" ]; then
    echo "Error: spec_version ($spec_version) does not match the expected version ($numeric_version)"
    exit 1
fi

if [ "$runtime_version" != "$version" ]; then
    echo "Error: runtime version ($runtime_version) does not match the expected version ($version)"
    exit 1
fi
