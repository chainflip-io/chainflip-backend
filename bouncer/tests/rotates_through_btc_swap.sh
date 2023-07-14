#!/bin/bash

echo "=== Testing BTC swaps through vault rotations ==="
MY_ADDRESS=`pnpm tsx ./commands/new_eth_address.ts foo` &&
echo "Generated ETH address " $MY_ADDRESS &&
pnpm tsx ./commands/new_swap.ts btc eth $MY_ADDRESS 100 &&
SWAP_ADDRESS=`pnpm tsx ./commands/observe_events.ts --timeout 10000 --succeed_on swapping:SwapDepositAddressReady --fail_on foo:bar | jq  -r ".[0].btc"` &&
SWAP_ADDRESS=`echo $SWAP_ADDRESS | xxd -r -p` &&
./tests/rotates_vaults.sh &&
OLD_BALKANCE=`pnpm tsx ./commands/get_eth_balance.ts $MY_ADDRESS` &&
pnpm tsx ./commands/fund_btc.ts $SWAP_ADDRESS 1 &&
pnpm tsx ./commands/observe_events.ts --timeout 60000 --succeed_on swapping:SwapExecuted --fail_on foo:bar > /dev/null &&
CONTINUE='no' &&
for i in `seq 60`; do
    NEW_BALANCE=`pnpm tsx ./commands/get_eth_balance.ts $MY_ADDRESS`
    if awk -v nb="$NEW_BALANCE" -v ob="$OLD_BALANCE" 'BEGIN {exit !(nb > ob)}'; then
        CONTINUE='yes'
        break
    else
        sleep 2
    fi
done &&
if [ "$CONTINUE" == "no" ]; then
    exit 1
fi &&
./tests/rotates_vaults.sh &&
MY_ADDRESS=`pnpm tsx ./commands/new_btc_address.ts bar` &&
echo "Generated BTC address " $MY_ADDRESS &&
pnpm tsx ./commands/new_swap.ts eth btc $MY_ADDRESS 100 &&
SWAP_ADDRESS=`pnpm tsx ./commands/observe_events.ts --timeout 10000 --succeed_on swapping:SwapDepositAddressReady --fail_on foo:bar | jq  -r ".[0].eth"` &&
OLD_BALANCE=`pnpm tsx ./commands/get_btc_balance.ts $MY_ADDRESS` &&
pnpm tsx ./commands/fund_eth.ts $SWAP_ADDRESS 1 &&
pnpm tsx ./commands/observe_events.ts --timeout 60000 --succeed_on swapping:SwapExecuted --fail_on foo:bar > /dev/null &&
for i in `seq 60`; do
	NEW_BALANCE=`pnpm tsx ./commands/get_btc_balance.ts $MY_ADDRESS`
    if awk -v nb="$NEW_BALANCE" -v ob="$OLD_BALANCE" 'BEGIN {exit !(nb > ob)}'; then
        echo "=== Test complete ==="
        exit 0
    else
        sleep 2
    fi
done
exit 1