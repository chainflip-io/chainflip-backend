#!/bin/sh

#### RUNNER (put this in chainflip-version, for example):

# This is kind of hacky but appears to work. IDK if we can guarantee the presence of something like jq that can parse json.
RUNTIME_SPEC_VERSION=$(curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0", "method": "state_getRuntimeVersion", "params": []}' http://localhost:9933 | grep -o "\"specVersion\":[^,}]*" | awk -F ':' '{print $2}')


if [ $RUNTIME_SPEC_VERSION -le 3 ]; then
    CMD=/usr/local/bin/0.5/chainflip-engine
else
    CMD=chainflip-engine
fi

$CMD "$@"