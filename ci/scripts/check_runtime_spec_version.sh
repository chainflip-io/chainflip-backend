#!/bin/bash

network=$1

runtime_spec_version_file="state-chain/runtime/src/lib.rs"

# Extract the spec_version from the Rust file and convert it to a comparable number
raw_version=$(grep -F '#[sp_version::runtime_version]' -A 10 $runtime_spec_version_file | grep 'spec_version' | awk '{print $2}' | tr -d ',')
# Convert version like 1_07_10 to proper semver by removing leading zeros
spec_version=$(echo $raw_version | sed 's/_\([0-9]\)/_\1/g' | tr -d '_')

if [ -z $network ]; then
    echo "Network not specified"
    exit 1
fi

if [ $network == "berghain" ]; then
    RPC_ENDPOINT="https://mainnet-archive.chainflip.io"
elif [ $network == "perseverance" ]; then
    RPC_ENDPOINT="https://archive.perseverance.chainflip.io"
elif [ $network == "sisyphos" ]; then
    RPC_ENDPOINT="https://archive.sisyphos.chainflip.io"
else
    echo "Invalid network"
    exit 1
fi

live_runtime_version=$(curl -s -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "state_getRuntimeVersion", "params":[]}' $RPC_ENDPOINT | jq .result.specVersion)

echo "Live runtime version: $live_runtime_version, Current Spec version: $spec_version (from $raw_version)"

# Convert live_runtime_version to the same format for comparison
live_runtime_version_comparable=$(printf "%d" $live_runtime_version)

# Compare versions
if [ $spec_version -gt $live_runtime_version_comparable ]; then
    echo "Runtime version has been incremented."
    exit 0
else
    echo "Runtime version has not been incremented."
    exit 2
fi
