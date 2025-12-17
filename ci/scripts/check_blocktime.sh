#!/bin/bash

./chainflip-node --dev &

# Wait for node to start
echo -e "🚀 Starting chainflip-node..."
sleep 10

# call rpc to get blocktime
# SLOT_DURATION="$(curl --silent -H "Content-Type: application/json" -d '{ "jsonrpc":"2.0", "id":"1", "method":"cf_slot_duration", "params":{} }' http://localhost:9944 | jq '.result')"
SLOT_DURATION="$(curl --silent --location 'http://localhost:9944' \
  --header 'Content-Type: application/json' \
  --data '{"id":9,"jsonrpc":"2.0","method":"state_call","params":["AuraApi_slot_duration",""]}' | jq -r '.result')"

# The rpc returns a scale encoded result, the two possible values are:
# - "0x7017000000000000" => 6000ms
# - "0xe803000000000000" => 1000ms

# we expect a block time of 6000ms on all live networks. ANYTHING ELSE WILL BREAK THE NETWORK.
if [[ $SLOT_DURATION == 0x7017000000000000 ]]; then
  echo -e "Slot duration is correct (6000ms)."
else
  echo -e "ERROR: Wrong slot duration: ${SLOT_DURATION}. Expected it to be 0x7017000000000000 (6000ms)"
  echo -e "Please make sure that the state-chain is compiled without the 'turbo' feature flag."
  exit 1
fi
