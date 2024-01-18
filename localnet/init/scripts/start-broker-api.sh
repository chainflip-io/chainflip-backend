#!/bin/bash
set -e
binary_location=$1

echo $($binary_location/chainflip-broker-api --version)

RUST_LOG=debug,jsonrpsee_types::params=trace $binary_location/chainflip-broker-api \
  --port=10997 \
  --state_chain.ws_endpoint=ws://localhost:9944 \
  --state_chain.signing_key_file $KEYS_DIR/BROKER_1 > /tmp/chainflip/chainflip-broker-api.log 2>&1 &
