#!/bin/bash
set -e
source ./localnet/init/env/eth.env
target/release/chainflip-node key insert --chain=dev --base-path=/tmp/chainflip/chaindata --suri=0x$(cat ./localnet/init/secrets/signing_key_file) --key-type=aura --scheme=sr25519
target/release/chainflip-node key insert --chain=dev --base-path=/tmp/chainflip/chaindata --suri=0x$(cat ./localnet/init/secrets/signing_key_file) --key-type=gran --scheme=ed25519
target/release/chainflip-node --chain=dev \
  --base-path=/tmp/chainflip/chaindata \
  --node-key-file=./localnet/init/secrets/keys/node_key_file \
  --validator \
  --force-authoring \
  --rpc-cors=all \
  --ws-external \
  --rpc-methods=Unsafe \
  --name=bashful \
  --state-cache-size=0 > /tmp/chainflip/chainflip-node.log 2>&1 &