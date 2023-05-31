#!/bin/bash

# INSTRUCTIONS
#
# This command takes two arguments.
# It will fund the Bitcoin address provided as the first argument with the amount of
# tokens provided in the second argument. The token amount is interpreted in BTC
#
# For example: ./commands/fund_btc.sh bcrt1ptdd9uy58dxf8ua9y7z0xf89y32qpmul600zgnmrga299vce8m9qq23ej63 1.2
# will send 1.2 BTC to account bcrt1ptdd9uy58dxf8ua9y7z0xf89y32qpmul600zgnmrga299vce8m9qq23ej63

if [ -z "${BTC_ENDPOINT}" ]; then
    export BTC_ENDPOINT="http://127.0.0.1:8332"
else
    echo "BTC_ENDPOINT is set to '$BTC_ENDPOINT'"
fi

bitcoin_address=$1
btc_amount=$2

echo "Sending $btc_amount BTC to $bitcoin_address"

result=`curl -s --user flip:flip  -X POST -H "Content-Type: text/plain" -d "{\"jsonrpc\":\"1.0\", \"id\":\"1\", \"method\":\"sendtoaddress\", \"params\": [\"$bitcoin_address\", $btc_amount, \"\", \"\", false, true, null, \"unset\", null, 1]}" $BTC_ENDPOINT/wallet/whale`
error=`echo $result | jq -r ".error"`
txid=`echo $result | jq -r ".result"`
if [ "$error" != "null" ]; then
    echo "ERROR: $error"
    exit -1
else
    for i in `seq 50`; do
        confirmations=`curl -s --user flip:flip  -X POST -H "Content-Type: text/plain" -d "{\"jsonrpc\":\"1.0\", \"id\":\"1\", \"method\":\"gettransaction\", \"params\": [\"$txid\"]}" $BTC_ENDPOINT/wallet/whale | jq -r ".result.confirmations"`
        if [ "$confirmations" -lt 1 ]; then
            sleep 1
        else
            exit 0
        fi
    done
fi
exit -1