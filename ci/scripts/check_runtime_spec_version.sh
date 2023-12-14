#!/bin/bash

network=$1

runtime_spec_version_file="state-chain/runtime/src/lib.rs"

# Extract the spec_version from the Rust file
spec_version=$(grep 'spec_version:' $runtime_spec_version_file | awk '{print $2}' | tr -d ',')

if [ -z $network ]; then
    echo "Network not specified"
    exit 1
fi

if [ $network == "berghain" ]; then
    RPC_ENDPOINT="https://mainnet-archive.chainflip.io"
else
    RPC_ENDPOINT="https://perseverance.chainflip.xyz"
fi

live_runtime_version=$(curl -s -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "state_getRuntimeVersion", "params":[]}' $RPC_ENDPOINT | jq .result.specVersion)

echo "Live runtime version: $live_runtime_version, Current Spec version: $spec_version"

# Compare versions
if [ $spec_version -gt $live_runtime_version ]; then
    echo "Runtime version has been incremented.
    exit 0
else
    echo "Runtime version has not been incremented.
    exit 2
fi
