#!/bin/bash
## Regenerates the bouncer's generated types (event schemas and dedot chaintypes) without a full
## localnet. The generators only read runtime metadata over RPC, so a bare dev node is enough.
## Usage: ./localnet/generate_bouncer_types.sh [binary_root_path] (default: ./target/debug)

set -e

BINARY_ROOT_PATH=${1:-./target/debug}
# Hardcoded as the localnet RPC url in @chainflip/processor.
RPC_URL=http://127.0.0.1:9944
LOG_FILE=/tmp/chainflip/bouncer-types-node.log
KEYS_DIR=./localnet/init/keys

rpc() {
  curl -sf -m 2 -H 'Content-Type: application/json' \
    -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"$1\",\"params\":[]}" $RPC_URL
}

if rpc system_health >/dev/null; then
  echo "❌ A node is already serving $RPC_URL. Destroy the localnet first, or run the generators in bouncer/commands/ against it."
  exit 1
fi

mkdir -p /tmp/chainflip
BASE_PATH=$(mktemp -d)
trap 'rm -rf "$BASE_PATH"' EXIT

# The node only serves the broker_* and lp_* RPCs when it holds their keys. The chaintypes include
# the RPC method list, so insert the same keys as a localnet node to match its output.
$BINARY_ROOT_PATH/chainflip-node key insert --chain=dev --base-path="$BASE_PATH" \
  "--suri=0x$(cat "$KEYS_DIR/BROKER_API")" --key-type=brok --scheme=sr25519
$BINARY_ROOT_PATH/chainflip-node key insert --chain=dev --base-path="$BASE_PATH" \
  "--suri=0x$(cat "$KEYS_DIR/LP_API")" --key-type=lqpr --scheme=sr25519

echo "🚧 Starting dev node (log: $LOG_FILE) ..."
# Non-default p2p port so it can't collide with a localnet node's. --validator and archive pruning
# enable the grandpa_ext_* and archive_v1_* RPCs, which a localnet node serves too.
$BINARY_ROOT_PATH/chainflip-node --chain=dev --base-path="$BASE_PATH" --rpc-port=9944 \
  --rpc-methods=unsafe --port=30433 --no-mdns --validator --unsafe-force-node-key-generation \
  --blocks-pruning=archive --state-pruning=archive >$LOG_FILE 2>&1 &
NODE_PID=$!
trap 'kill $NODE_PID 2>/dev/null; wait $NODE_PID 2>/dev/null; rm -rf "$BASE_PATH"' EXIT

for _ in $(seq 60); do
  rpc system_health >/dev/null && break
  kill -0 $NODE_PID 2>/dev/null || { echo "❌ Node exited, see $LOG_FILE"; exit 1; }
  sleep 1
done
rpc system_health >/dev/null || { echo "❌ Node RPC not up after 60s, see $LOG_FILE"; exit 1; }

# Both generators already emit prettier-formatted output, matching what CI checks.
cd bouncer
echo "📐 Updating event schemas ..."
./commands/generate_event_schemas.ts
echo "📐 Updating chaintypes ..."
./commands/generate_chaintypes.ts
echo "✅ Updated bouncer/generated/events and bouncer/generated/chaintypes"
