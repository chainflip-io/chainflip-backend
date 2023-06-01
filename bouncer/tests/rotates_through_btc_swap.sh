#!/bin/bash

echo "=== Testing BTC swaps through vault rotations ==="
MY_ADDRESS=`./commands/new_dot_address.sh foo` &&
echo "Generated DOT address " $MY_ADDRESS &&
./commands/new_swap.sh btc dot $MY_ADDRESS 100 &&
SWAP_ADDRESS=`./commands/observe_events.sh --timeout 10000 --succeed_on swapping:SwapDepositAddressReady --fail_on foo:bar | jq  -r ".[0].btc"` &&
SWAP_ADDRESS=`echo $SWAP_ADDRESS | xxd -r -p` &&
./tests/rotates_vaults.sh &&
OLD_BALKANCE=`./commands/get_dot_balance.sh $MY_ADDRESS` &&
./commands/fund_btc.sh $SWAP_ADDRESS 1 &&
./commands/observe_events.sh --timeout 60000 --succeed_on swapping:SwapExecuted --fail_on foo:bar > /dev/null &&
CONTINUE='no' &&
for i in `seq 60`; do
    NEW_BALANCE=`./commands/get_dot_balance.sh $MY_ADDRESS`
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
MY_ADDRESS=`./commands/new_btc_address.sh bar` &&
echo "Generated BTC address " $MY_ADDRESS &&
./commands/new_swap.sh dot btc $MY_ADDRESS 100 &&
SWAP_ADDRESS=`./commands/observe_events.sh --timeout 10000 --succeed_on swapping:SwapDepositAddressReady --fail_on foo:bar | jq  -r ".[0].dot"` &&
OLD_BALANCE=`./commands/get_btc_balance.sh $MY_ADDRESS` &&
./commands/fund_dot.sh $SWAP_ADDRESS 1000 &&
./commands/observe_events.sh --timeout 60000 --succeed_on swapping:SwapExecuted --fail_on foo:bar > /dev/null &&
for i in `seq 60`; do
	NEW_BALANCE=`./commands/get_btc_balance.sh $MY_ADDRESS`
    if awk -v nb="$NEW_BALANCE" -v ob="$OLD_BALANCE" 'BEGIN {exit !(nb > ob)}'; then
        echo "=== Test complete ==="
        exit 0
    else
        sleep 2
    fi
done
exit 1