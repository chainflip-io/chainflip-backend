#!/bin/bash
set -e

echo "🚧 Starting chainflip-node(s) ..."

echo "start-all-nodes INIT_RPC_PORT: $INIT_RPC_PORT"

echo "start-all-nodes CHAIN: $CHAIN"
echo "start-all-nodes KEYS_DIR: $KEYS_DIR"
echo "start-all-nodes LOCALNET_INIT_DIR: $LOCALNET_INIT_DIR"
echo "start-all-nodes NODE_COUNT: $NODE_COUNT"
echo "start-all-nodes SELECTED_NODES: $SELECTED_NODES"
echo "start-all-nodes INIT_RPC_PORT: $INIT_RPC_PORT"

P2P_PORT=30333
RPC_PORT=$INIT_RPC_PORT
for NODE in $SELECTED_NODES; do
    echo "🚧 Starting chainflip-node of $NODE ..."

    CHAIN=$CHAIN KEYS_DIR=$KEYS_DIR LOCALNET_INIT_DIR=$LOCALNET_INIT_DIR $LOCALNET_INIT_DIR/scripts/start-node.sh $BINARY_ROOT_PATH $NODE $P2P_PORT $RPC_PORT $NODE_COUNT
    ((P2P_PORT++))
    ((RPC_PORT++))
done
