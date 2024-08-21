#!/bin/bash

# RPC endpoint
ENDPOINT="http://localhost:9944"

# Check if RPC server is up by sending a simple request
RESPONSE=$(curl --max-time 30 -s -o /dev/null -w "%{http_code}" $ENDPOINT/health)

# If the server responds with a 200 status code, it is live
if [ "$RESPONSE" -eq 200 ]; then
  exit 0
else
  exit 1
fi
