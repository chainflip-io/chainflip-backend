#!/bin/sh

RUNTIME_SPEC_VERSION=$(curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "state_getRuntimeVersion", "params": []}' http://localhost:9933 | grep -o "\"specVersion\":[^,}]*" | awk -F ':' '{print $2}')

if [ $RUNTIME_SPEC_VERSION -le 3 ]; then
    CMD=/usr/local/bin/0.5/chainflip-engine
else
    CMD=chainflip-engine
fi

$CMD "$@"