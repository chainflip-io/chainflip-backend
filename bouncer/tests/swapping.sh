#!/bin/bash

get_balance() {
	GET_BALANCE_CCY=$1
	GET_BALANCE_ADDRESS=$2
	[ $GET_BALANCE_CCY == "btc"  ] && ./commands/get_btc_balance.sh  $GET_BALANCE_ADDRESS
	[ $GET_BALANCE_CCY == "eth"  ] && ./commands/get_eth_balance.sh  $GET_BALANCE_ADDRESS
	[ $GET_BALANCE_CCY == "dot"  ] && ./commands/get_dot_balance.sh  $GET_BALANCE_ADDRESS
	[ $GET_BALANCE_CCY == "usdc" ] && ./commands/get_usdc_balance.sh $GET_BALANCE_ADDRESS
}

fund() {
	FUND_CCY=$1
	FUND_ADDRESS=$2
	[ $FUND_CCY == "btc"  ] && ./commands/fund_btc.sh  $FUND_ADDRESS 0.5
	[ $FUND_CCY == "eth"  ] && ./commands/fund_eth.sh  $FUND_ADDRESS 5
	[ $FUND_CCY == "dot"  ] && ./commands/fund_dot.sh  $FUND_ADDRESS 50
	[ $FUND_CCY == "usdc" ] && ./commands/fund_usdc.sh $FUND_ADDRESS 500
}

perform_swap() {
	SRC_CCY=$1
	DST_CCY=$2
	ADDRESS=$3
	FEE=100
	./commands/new_swap.sh $SRC_CCY $DST_CCY $ADDRESS $FEE
	DEPOSIT_ADDRESS_CCY=$SRC_CCY
	if [ "$SRC_CCY" == "usdc" ]; then
		DEPOSIT_ADDRESS_CCY="eth"
	fi
	SWAP_ADDRESS=`./commands/observe_events.sh --timeout 10000 --succeed_on swapping:SwapDepositAddressReady --fail_on foo:bar | jq  -r ".[0].$DEPOSIT_ADDRESS_CCY"`
	if [ "$SRC_CCY" == "btc" ]; then
		SWAP_ADDRESS=`echo $SWAP_ADDRESS | xxd -r -p`
	fi
	OLD_BALANCE=`get_balance $DST_CCY $ADDRESS`
	fund $SRC_CCY $SWAP_ADDRESS
	./commands/observe_events.sh --timeout 30000 --succeed_on swapping:SwapExecuted --fail_on foo:bar > /dev/null &&
	for i in `seq 60`; do
		NEW_BALANCE=`get_balance $DST_CCY $ADDRESS`
	    if awk -v nb="$NEW_BALANCE" -v ob="$OLD_BALANCE" 'BEGIN {exit !(nb > ob)}'; then
	        return 0
	    else
	        sleep 2
	    fi
	done
	exit 1
}
echo "=== Testing all swap combinations ===" &&
MY_ADDRESS=`./commands/new_btc_address.sh never P2PKH` &&
echo "Created new BTC address $MY_ADDRESS" &&
perform_swap dot btc $MY_ADDRESS &&
MY_ADDRESS=`./commands/new_btc_address.sh gonna P2SH` &&
echo "Created new BTC address $MY_ADDRESS" &&
perform_swap eth btc $MY_ADDRESS &&
MY_ADDRESS=`./commands/new_btc_address.sh give P2WPKH` &&
echo "Created new BTC address $MY_ADDRESS" &&
perform_swap usdc btc $MY_ADDRESS &&
MY_ADDRESS=`./commands/new_btc_address.sh you P2WSH` &&
echo "Created new BTC address $MY_ADDRESS" &&
perform_swap dot btc $MY_ADDRESS &&
MY_ADDRESS=`./commands/new_dot_address.sh up` &&
echo "Created new DOT address $MY_ADDRESS" &&
perform_swap btc dot $MY_ADDRESS &&
MY_ADDRESS=`./commands/new_eth_address.sh and` &&
echo "Created new USDC address $MY_ADDRESS" &&
perform_swap dot usdc $MY_ADDRESS
MY_ADDRESS=`./commands/new_eth_address.sh desert` &&
echo "Created new ETH address $MY_ADDRESS" &&
perform_swap btc eth $MY_ADDRESS &&
echo "=== Test complete ==="


