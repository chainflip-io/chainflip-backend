#!/bin/sh

# This command prepares the localnet for a Bitcoin reorg. It will stop generating bitcoin blocks,
# so commands that wait for the success of a bitcoin transaction need to be run asynchronously after
# executing "prepare_reorg.sh". After running this command and submitting the BTC transactions to be included
# in the reorg, run "execute_regorg.sh <REORG_DEPTH>"

echo "Launching second BTC node"
docker run --platform linux/amd64 -d --network=chainflip-localnet_default --name secondary_btc_node ghcr.io/chainflip-io/chainflip-bitcoin-regtest:v24.0.1 bash start.sh dont_mine > /dev/null
while ! docker exec secondary_btc_node bitcoin-cli getblockchaininfo > /dev/null 2>&1; do
    sleep 1
done

echo "Disabling mining on main BTC node"
docker exec bitcoin rm /root/mine_blocks

echo "Synchronizing nodes"
docker exec secondary_btc_node bitcoin-cli addnode "bitcoin" "onetry"
BLOCKS=$(docker exec bitcoin bitcoin-cli getblockcount)
while [ $(docker exec secondary_btc_node bitcoin-cli getblockcount) != $BLOCKS ]; do
    sleep 1
done

echo "Disconnecting nodes"
docker exec secondary_btc_node bitcoin-cli disconnectnode "bitcoin"
sleep 1