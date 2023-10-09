#!/bin/bash
set -e
BINARY_PATH=$1
NODE_NAME=$2
PORT=$3
RPC_PORT=$4
NODE_COUNT=$5

CHAIN="dev"
if [ $NODE_COUNT == "3-node" ]; then
    CHAIN="test"
fi

source ./localnet/init/env/eth.env
source ./localnet/init/env/arb.env
source ./localnet/init/env/node.env
export ETH_INIT_AGG_KEY=$(jq -r '.eth_agg_key' ./localnet/init/keyshare/$NODE_COUNT/agg_keys.json)
export DOT_INIT_AGG_KEY=$(jq -r '.dot_agg_key' ./localnet/init/keyshare/$NODE_COUNT/agg_keys.json)
$BINARY_PATH/chainflip-node key insert --chain=$CHAIN --base-path=/tmp/chainflip/$NODE_NAME/chaindata --suri=0x$(cat ./localnet/init/keys/$NODE_NAME/signing_key_file) --key-type=aura --scheme=sr25519
$BINARY_PATH/chainflip-node key insert --chain=$CHAIN --base-path=/tmp/chainflip/$NODE_NAME/chaindata --suri=0x$(cat ./localnet/init/keys/$NODE_NAME/signing_key_file) --key-type=gran --scheme=ed25519
$BINARY_PATH/chainflip-node --chain=$CHAIN \
  --base-path=/tmp/chainflip/$NODE_NAME/chaindata \
  --node-key-file=./localnet/init/keys/$NODE_NAME/node_key_file \
  --validator \
  --force-authoring \
  --rpc-cors=all \
  --unsafe-rpc-external \
  --rpc-methods=unsafe \
  --name=$NODE_NAME \
  --port=$PORT \
  --rpc-port=$RPC_PORT \
  --blocks-pruning=archive \
  --state-pruning=archive \
  --trie-cache-size=0 > /tmp/chainflip/$NODE_NAME/chainflip-node.log 2>&1 &
