#!/bin/bash
set -e
chainflip-node key insert --chain=dev --base-path=/etc/chainflip/chaindata --suri=0x${SURI} --key-type=aura --scheme=sr25519
chainflip-node key insert --chain=dev --base-path=/etc/chainflip/chaindata --suri=0x${SURI} --key-type=gran --scheme=ed25519
chainflip-node --chain=dev \
  --base-path=/etc/chainflip/chaindata \
  --node-key-file=/etc/chainflip/keys/node_key_file \
  --validator \
  --force-authoring \
  --rpc-cors=all \
  --ws-external \
  --rpc-methods=Unsafe \
  --name=bashful \
  --state-cache-size=0