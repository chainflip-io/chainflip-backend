#!/bin/bash

# INSTRUCTIONS
#
# This command takes one argument.
# It will print the Bitcoin balance of the address provided as the first argument.
#
# For example: ./commands/get_btc_balance.sh bcrt1ptdd9uy58dxf8ua9y7z0xf89y32qpmul600zgnmrga299vce8m9qq23ej63
# might print: 1.2

if [ -z "${BTC_ENDPOINT}" ]; then
    export BTC_ENDPOINT="http://127.0.0.1:8332"
else
    echo "BTC_ENDPOINT is set to '$BTC_ENDPOINT'"
fi

bitcoin_address=$1

if [ -z "$bitcoin_address" ]; then
    echo "Must provide an address to query"
    exit -1
fi
result=`curl -s --user flip:flip  -X POST -H "Content-Type: text/plain" -d "{\"jsonrpc\":\"1.0\", \"id\":\"1\", \"method\":\"listreceivedbyaddress\", \"params\": [1, false, true, \"$bitcoin_address\"]}" $BTC_ENDPOINT/wallet/watch`
error=`echo $result | jq -r ".error"`
amount=`echo $result | jq -r ".result[0].amount"`
if [ "$error" != "null" ]; then
    echo "ERROR: $error"
    exit -1
else
    if [ "$amount" != "null" ]; then
        echo $amount
    else
        echo 0
    fi
fi
exit 0