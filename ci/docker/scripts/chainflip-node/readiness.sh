#!/bin/bash

# RPC endpoint
ENDPOINT="http://localhost:9944"

# Check node health using the system_health RPC method
HEALTH=$(curl -s -X POST --max-time 30 --header "Content-Type: application/json" --data '{"jsonrpc":"2.0","method":"system_health","params":[],"id":1}' $ENDPOINT)

# Parse isSyncing status using jq (make sure jq is installed in the image)
IS_SYNCING=$(echo $HEALTH | jq -r '.result.isSyncing')

# If isSyncing is false, the node is fully synced and ready
if [ "$IS_SYNCING" = "false" ]; then
  exit 0
else
  exit 1
fi
