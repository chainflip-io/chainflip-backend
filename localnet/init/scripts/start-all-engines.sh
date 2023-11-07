# Starts all the engines necessary for the network, or for an upgrade.

# These need to match what's in the manage.py script.
SC_RPC_PORT=9944
HEALTH_PORT=5555

ENGINE_P2P_PORT=3100
LOG_PORT=30687
for NODE in $SELECTED_NODES; do
    cp -R $LOCALNET_INIT_DIR/keyshare/$NODE_COUNT/$NODE.db /tmp/chainflip/$NODE
    BINARY_ROOT_PATH=$BINARY_ROOT_PATH NODE_NAME=$NODE P2P_PORT=$ENGINE_P2P_PORT SC_RPC_PORT=$SC_RPC_PORT LOG_PORT=$LOG_PORT HEALTH_PORT=$HEALTH_PORT LOG_SUFFIX=$LOG_SUFFIX $LOCALNET_INIT_DIR/scripts/start-engine.sh
    echo "🚗 Starting chainflip-engine of $NODE ..."
    ((SC_RPC_PORT++))
    ((ENGINE_P2P_PORT++))
    ((HEALTH_PORT++))
    ((LOG_PORT++))
done