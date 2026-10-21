#!/bin/bash
## Regenerates the bouncer event schemas without a full localnet.
## The generator only reads runtime metadata over RPC, so a bare dev node is enough.
## Usage: ./localnet/generate_event_schemas.sh [binary_root_path] (default: ./target/debug)

set -e

BINARY_ROOT_PATH=${1:-./target/debug}
# Hardcoded as the localnet RPC url in @chainflip/processor.
RPC_URL=http://127.0.0.1:9944
LOG_FILE=/tmp/chainflip/schema-node.log

rpc() {
  curl -sf -m 2 -H 'Content-Type: application/json' \
    -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"$1\",\"params\":[]}" $RPC_URL
}

if rpc system_health >/dev/null; then
  echo "❌ A node is already serving $RPC_URL. Destroy the localnet first, or run ./commands/generate_event_schemas.ts from bouncer/ to generate against it."
  exit 1
fi

mkdir -p /tmp/chainflip
echo "🚧 Starting dev node (log: $LOG_FILE) ..."
# Non-default p2p port so it can't collide with a localnet node's.
$BINARY_ROOT_PATH/chainflip-node --chain=dev --tmp --rpc-port=9944 --port=30433 --no-mdns >$LOG_FILE 2>&1 &
NODE_PID=$!
trap 'kill $NODE_PID 2>/dev/null' EXIT

for _ in $(seq 60); do
  rpc system_health >/dev/null && break
  kill -0 $NODE_PID 2>/dev/null || { echo "❌ Node exited, see $LOG_FILE"; exit 1; }
  sleep 1
done
rpc system_health >/dev/null || { echo "❌ Node RPC not up after 60s, see $LOG_FILE"; exit 1; }

echo "📐 Updating event schemas ..."
cd bouncer
./commands/generate_event_schemas.ts
pnpm prettier --write generated/events >/dev/null
echo "✅ Event schemas updated in bouncer/generated/events"
