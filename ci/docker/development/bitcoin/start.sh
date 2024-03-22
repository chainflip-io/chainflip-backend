#!/bin/bash
set -x
btc_version=$(bitcoind --version)
btc_block_time=5

PRUNE=$1

if [ "$PRUNE" == "true" ]; then
  bitcoind -debug=http -debug=rpc &
else
  bitcoind -debug=http -debug=rpc -prune=0 &

  while [ "$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:8332)" != "405" ]; do
    echo "Waiting for bitcoind to start..."
    sleep 1
  done

  electrs --conf electrs.conf &
fi

echo "Bitcoin version: $btc_version"
echo "Bitcoin block time: $btc_block_time"
echo "Generating a block every $btc_block_time seconds."
BTCEXP_BITCOIND_USER=flip BTCEXP_HOST=0.0.0.0 BTCEXP_BITCOIND_PASS=flip BTCEXP_SLOW_DEVICE_MODE=false BTCEXP_RPC_ALLOWALL=true BTCEXP_BASIC_AUTH_PASSWORD=flip btc-rpc-explorer &
sleep 5

whale_wallet_name="whale"
watch_wallet_name="watch"

if ! ls /root/.bitcoin/regtest/wallets/$whale_wallet_name 1> /dev/null 2>&1; then
    echo "Creating wallet $whale_wallet_name"
    bitcoin-cli createwallet $whale_wallet_name false false "" false true true false
fi
if ! ls /root/.bitcoin/regtest/wallets/$watch_wallet_name 1> /dev/null 2>&1; then
    echo "Creating wallet $watch_wallet_name"
    bitcoin-cli createwallet $watch_wallet_name true false "" false true true false # set no private key to true
fi

address=$(bitcoin-cli -rpcwallet=$whale_wallet_name getnewaddress)

if [ ! $1 ] || [ $1 != "dont_mine" ]; then
    bitcoin-cli generatetoaddress 101 $address
    touch /root/mine_blocks
fi

while :
do
    if [ -e /root/mine_blocks ]; then
        echo "Generate a new block `date '+%d/%m/%Y %H:%M:%S'`"
        bitcoin-cli generatetoaddress 1 $address
    fi
    sleep $btc_block_time
done
