#!/bin/bash

# Node RPC endpoint
NODE_RPC_URL="http://localhost:9944"

# Initialize counters
counter=0
connection_attempts=0

# Define color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Other initializations...
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
  response=$(curl -s -H "Content-Type: application/json" --data '{"jsonrpc":"2.0", "method":"system_health", "params":[], "id":1}' $NODE_RPC_URL)
  if [[ -z "$response" ]]; then
    # Increment connection_attempts if curl command fails (no response)
    ((connection_attempts++))
    echo -e "${YELLOW}Warning: Attempt $connection_attempts failed to connect to the node.${NC}"

    if [ "$connection_attempts" -ge 10 ]; then
      echo -e "${RED}🚨 Error: Failed to connect to the node after 10 attempts.${NC}"
      cleanup
      exit 1
    fi
  else
    # Reset connection_attempts counter on successful connection
    connection_attempts=0
    peers=$(echo $response | jq -r '.result.peers // empty')

    # Proceed with the rest of your script...
    if ! [[ $peers =~ ^[0-9]+$ ]]; then
      echo -e "${RED}⚠️ Failed to fetch the number of peers or received an invalid response.${NC}"
    elif [ "$peers" -eq 0 ]; then
      ((counter++))
      echo -e "${YELLOW}🚫 No peers found, counter=$counter${NC}"
    else
      if [ $counter -ne 0 ]; then
        echo -e "${GREEN}✅ Success: Peers are now connected after $counter seconds of no connections.${NC}"
      else
        echo -e "${GREEN}✅ Success: Peers are connected.${NC}"
      fi
      cleanup
      exit 0
    fi

    if [ "$counter" -ge 6 ]; then
      echo -e "${RED}🚨 Error: No peers connected for 60 seconds.${NC}"
      cleanup
      exit 1
    fi
  fi
  # Sleep for 10 second before the next check
  sleep 10
done
