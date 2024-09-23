#!/bin/bash
## Stops the existing localnet and starts a new one using the same settings. 
## The settings are saved in the settings.sh file in the tmp directory.

source ./localnet/common.sh

# Load the env vars that the last network used
load_settings
if [ -z "$NODE_COUNT" ] || [ -z "$BINARY_ROOT_PATH" ]; then
  echo "❌ Error: no existing network to recreate. Please run the manage script first to build a network."
  exit 1
fi

# Destroy and start a new network
destroy
sleep 5
build-localnet