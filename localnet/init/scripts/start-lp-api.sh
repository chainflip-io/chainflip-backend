#!/bin/bash
set -e
DATETIME=$(date '+%Y-%m-%d_%H-%M-%S')
binary_location=$1
RUST_LOG=debug,jsonrpsee_types::params=trace $binary_location/chainflip-lp-api \
  --port=10589 \
  --state_chain.ws_endpoint=ws://localhost:9944 \
  --state_chain.signing_key_file $KEYS_DIR/LP_1 > /tmp/chainflip/chainflip-lp-api.$DATETIME.log 2>&1 &
