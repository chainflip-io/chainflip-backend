#!/bin/bash

# Initialize variables
NETWORK=""
BINARY_ROOT_PATH=""

# Define color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default Node RPC endpoint
NODE_RPC_URL="http://localhost:9944"

# Initialize counter
counter=0

CHAINDATA_PATH="/tmp/chainflip/chaindata"
SYNC_MODE="warp"
REQUIRED_BINARIES="chainflip-node"

# Parse command-line arguments
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --network) NETWORK="$2"; shift ;;
        --binary-root-path) BINARY_ROOT_PATH="$2"; shift ;;
        *) echo -e "${RED}Unknown parameter passed: $1${NC}"; exit 1 ;;
    esac
    shift
done

CHAINSPEC_PATH="./state-chain/node/chainspecs/${NETWORK}.chainspec.raw.json"

function cleanup {
  echo "🧹 Cleaning up..."
  for pid in $(ps -ef | grep chainflip | grep -v grep | awk '{print $2}'); do kill -9 $pid; done
  echo "🪵 Printing Node logs ..."
  cat /tmp/chainflip-node.log
  rm -rf $CHAINDATA_PATH
}

# Checks
if [ -z "$NETWORK" ]; then
  echo -e "${RED}❌ Error: Network name is required (--network).${NC} Options are: (sisyphos, perseverance, berghain)"
  exit 1
fi

if [ -z "$BINARY_ROOT_PATH" ]; then
  echo -e "${RED}❌ Error: Binary root path is required (--binary-root-path).${NC}"
  exit 1
fi

for binary in $REQUIRED_BINARIES; do
  if [[ ! -f $BINARY_ROOT_PATH/$binary ]]; then
    echo -e "${RED}❌ Couldn't find $binary at $BINARY_ROOT_PATH.${NC}"
    exit 1
  fi
done

if [[ ! -f $CHAINSPEC_PATH ]]; then
  echo -e "${RED}❌ Couldn't find chainspec at $CHAINSPEC_PATH.${NC}"
  exit 1
fi

mkdir -p $CHAINDATA_PATH

# Purge chaindata directory
rm -rf $CHAINDATA_PATH/*

#  Start chaonflip-node
$BINARY_ROOT_PATH/chainflip-node \
    --chain=$CHAINSPEC_PATH \
    --base-path=$CHAINDATA_PATH --sync=$SYNC_MODE &> /tmp/chainflip-node.log &
# Wait for node to start
echo -e "${CYAN}🚀 Starting chainflip-node...${NC}"
sleep 10

while true; do
  # Fetch the number of peers using system_health RPC call
  peers=$(curl -s -H "Content-Type: application/json" --data '{"jsonrpc":"2.0", "method":"system_health", "params":[], "id":1}' $NODE_RPC_URL | jq -r '.result.peers // empty')

  # Check if the peers variable is a number
  if ! [[ $peers =~ ^[0-9]+$ ]]; then
    echo -e "${RED}⚠️ Failed to fetch the number of peers or received an invalid response.${NC}"
    # Optionally, you can choose to break or exit here if consistent fetching is critical
    # exit 1
  elif [ "$peers" -eq 0 ]; then
    # Increment counter if no peers
    ((counter++))
    echo -e "${YELLOW}🚫 No peers found, counter=$counter${NC}"
  else
    # Reset counter and indicate success when peers are connected
    if [ $counter -ne 0 ]; then
      echo -e "${GREEN}✅ Success: Peers are now connected after $counter seconds of no connections.${NC}"
      cleanup
      exit 0
    else
      echo -e "${GREEN}✅ Success: Peers are connected.${NC}"
      cleanup
      exit 0
    fi
  fi

  # Check if counter has reached 60 seconds (1 minute)
  if [ "$counter" -ge 60 ]; then
    echo -e "${RED}🚨 Error: No peers connected for 60 seconds.${NC}"
    cleanup
    exit 1
  fi
  # Sleep for 1 second before the next check
  sleep 1
done
