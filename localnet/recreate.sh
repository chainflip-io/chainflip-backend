#!/bin/bash
## Stops the existing localnet (if running) and starts a new one using the same settings. 
## The settings are saved in the settings.sh file in the tmp directory on creation of a localnet.
## Use the -d flag to use default values if no settings file is found.

# Parse arguments
USE_DEFAULTS=false
while getopts "dh" opt; do
  case $opt in
    d) USE_DEFAULTS=true;;
    h) echo "Use -d to create with deafult values if no settings file is found"; exit 0;;
    \?) echo "Invalid option -$OPTARG" >&2 ; exit 0 ;;
  esac
done

source ./localnet/common.sh

# Load the env vars that the last localnet used
load_settings

# Use default values or error if no settings file was found
if [ -z "$NODE_COUNT" ] || [ -z "$BINARY_ROOT_PATH" ]; then
  if [ "$USE_DEFAULTS" = true ]; then
    export BINARY_ROOT_PATH="./target/debug"
    export NODE_COUNT="1-node"
    export START_TRACKER="NO"
    echo "No settings file found. Using default values:"
    echo "BINARY_ROOT_PATH: $BINARY_ROOT_PATH"
    echo "NODE_COUNT: $NODE_COUNT"
    echo "START_TRACKER: $START_TRACKER"
  else
    echo "❌ Error: no settings file found. Use -d to create one with defaults, or you can create one using the create/manage scripts."
    exit 1
  fi
fi

# Destroy and start a new localnet
destroy
sleep 5
build-localnet