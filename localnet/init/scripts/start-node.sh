#!/bin/bash
set -e
binary_location=$1
source ./localnet/init/env/eth.env
source ./localnet/init/env/arb.env
source ./localnet/init/env/node.env
$binary_location/chainflip-node key insert --chain=dev --base-path=/tmp/chainflip/chaindata --suri=0x$(cat ./localnet/init/keys/signing_key_file) --key-type=aura --scheme=sr25519
$binary_location/chainflip-node key insert --chain=dev --base-path=/tmp/chainflip/chaindata --suri=0x$(cat ./localnet/init/keys/signing_key_file) --key-type=gran --scheme=ed25519
$binary_location/chainflip-node --chain=dev \
  --base-path=/tmp/chainflip/chaindata \
  --node-key-file=./localnet/init/keys/node_key_file \
  --validator \
  --force-authoring \
  --rpc-cors=all \
  --ws-external \
  --rpc-methods=Unsafe \
  --name=bashful \
  --execution=Native \
  --blocks-pruning=archive \
  --state-pruning=archive \
  --state-cache-size=0 > /tmp/chainflip/chainflip-node.log 2>&1 &
