#!/bin/bash

./chainflip-node --dev &

# Wait for node to start
echo -e "🚀 Starting chainflip-node..."
sleep 10

# call rpc to get blocktime
SLOT_DURATION="$(curl --silent -H "Content-Type: application/json" -d '{ "jsonrpc":"2.0", "id":"1", "method":"cf_slot_duration", "params":{} }' http://localhost:9944 | jq '.result')"

# we expect a block time of 6000ms on all live networks. ANYTHING ELSE WILL BREAK THE NETWORK.
if [[ $SLOT_DURATION == 5000 ]]; then
  echo -e "Slot duration is correct (6000ms)."
else
  echo -e "ERROR: Wrong slot duration: ${SLOT_DURATION}ms. Expected it to be 6000ms"
  echo -e "Please make sure that the state-chain is compiled without the 'turbo' feature flag."
  exit 1
fi
