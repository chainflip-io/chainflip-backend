#!/bin/bash
set -e
BINARY_PATH=$1
NODE_NAME=$2
PORT=$3
HEALTH_PORT=$4
RPC_PORT=$5
export LOG_PORT=$6
source ./localnet/init/env/cfe.env
$BINARY_PATH/chainflip-engine \
  --config-root=./localnet/init/ \
  --eth.private_key_file=./localnet/init/keys/$NODE_NAME/eth_private_key_file \
  --state_chain.signing_key_file=./localnet/init/keys/$NODE_NAME/signing_key_file \
  --state_chain.ws_endpoint=ws://localhost:$RPC_PORT \
  --p2p.node_key_file=./localnet/init/keys/$NODE_NAME/node_key_file \
  --p2p.port=$PORT \
  --signing.db_file=/tmp/chainflip/$NODE_NAME/$NODE_NAME.db \
  --health_check.port=$HEALTH_PORT > /tmp/chainflip/$NODE_NAME/chainflip-engine.log 2>&1 &
