#!/bin/bash

# LOG_SUFFIX is optional. It can be used to differentiate between the logs of two engines, potentially with the same owner
# e.g. in the case of an upgrade where we run two engines simultaneously.


SCRIPT_DIR=$(dirname "$(realpath "$BASH_SOURCE")")
OLD_ENGINE_DYLIB_PATH="${SCRIPT_DIR}/../../../old-engine-dylib/"

echo "Starting engine..."
if [ -n "$DYLD_LIBRARY_PATH" ]; then
    export DYLD_LIBRARY_PATH="$DYLD_LIBRARY_PATH:$OLD_ENGINE_DYLIB_PATH"
else
    export DYLD_LIBRARY_PATH="$OLD_ENGINE_DYLIB_PATH"
fi

if [ -n "$LD_LIBRARY_PATH" ]; then
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$OLD_ENGINE_DYLIB_PATH"
else
    export LD_LIBRARY_PATH="$OLD_ENGINE_DYLIB_PATH"
fi

echo "DYLD_LIBRARY_PATH/LD_LIBRARY_PATH to find the engine dylib: $DYLD_LIBRARY_PATH"

set -e
DATETIME=$(date '+%Y-%m-%d_%H-%M-%S')
source $LOCALNET_INIT_DIR/env/cfe.env
$BINARY_ROOT_PATH/engine-runner \
  --config-root=$LOCALNET_INIT_DIR \
  --eth.private_key_file=./keys/$NODE_NAME/eth_private_key_file \
  --arb.private_key_file=./keys/$NODE_NAME/eth_private_key_file \
  --state_chain.signing_key_file=./keys/$NODE_NAME/signing_key_file \
  --state_chain.ws_endpoint=ws://localhost:$SC_RPC_PORT \
  --p2p.node_key_file=./keys/$NODE_NAME/node_key_file \
  --p2p.port=$P2P_PORT \
  --logging.command_server_port=$LOG_PORT \
  --signing.db_file=/tmp/chainflip/$NODE_NAME/$NODE_NAME.db \
  --health_check.port=$HEALTH_PORT > /tmp/chainflip/$NODE_NAME/chainflip-engine$LOG_SUFFIX.$DATETIME.log 2>&1 &
