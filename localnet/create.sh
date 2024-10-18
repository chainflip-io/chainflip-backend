#!/bin/bash

## Starts a localnet with the provided settings or uses default values if none provided
## 
## Usage ./localnet/create.sh -b <BINARY_ROOT_PATH> -n <NODE_COUNT> -t <START_TRACKER>
## Example ./localnet/create.sh -b ./target/debug -n 1 -s y

# Parse command-line arguments
while getopts "b:n:t:h" opt; do
  case $opt in
    b) BINARY_ROOT_PATH=$OPTARG ;;
    n) NODE_COUNT=$OPTARG ;;
    t) START_TRACKER=$OPTARG ;;
    h) echo "Usage: ./localnet/create.sh -b <BINARY_ROOT_PATH> -n <NODE_COUNT> -t <START_TRACKER>"; exit 0 ;;
    \?) echo "Invalid option -$OPTARG" >&2 ; exit 0 ;;
  esac
done
if [[ -n "$NODE_COUNT" && "$NODE_COUNT" != "1" && "$NODE_COUNT" != "3" ]]; then
  echo "❌ Invalid NODE_COUNT value: $NODE_COUNT"
  exit 1
fi
if [[ -n "$START_TRACKER" && "$START_TRACKER" != "y" && "$START_TRACKER" != "" ]]; then
  echo "❌ Invalid START_TRACKER value: $START_TRACKER"
  exit 1
fi

source ./localnet/common.sh

# Set default values if not provided
export BINARY_ROOT_PATH=${BINARY_ROOT_PATH:-"./target/debug"}
export NODE_COUNT=${NODE_COUNT:-"1-node"}
export START_TRACKER=${START_TRACKER:-"NO"}

# Print the values of the variables being used
echo "Using the following settings:"
echo "BINARY_ROOT_PATH: $BINARY_ROOT_PATH"
echo "NODE_COUNT: $NODE_COUNT"
echo "START_TRACKER: $START_TRACKER"

build-localnet